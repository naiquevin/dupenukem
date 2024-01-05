use crate::fileutil;
use log::warn;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Traverses the `dirpath` recursively using breadth first search
/// approach and returns a vector of `PathBuf`.
///
/// Optionally, a hashset of `PathBuf` refs can be passed as the
/// `excludes` arg. These paths will be excluded during traversal.
fn traverse_bfs(dirpath: &Path, excludes: Option<&HashSet<PathBuf>>) -> io::Result<Vec<PathBuf>> {
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    let mut result: Vec<PathBuf> = Vec::new();
    queue.push_back(dirpath.to_path_buf());
    loop {
        match queue.pop_front() {
            Some(p) => {
                for entry in fs::read_dir(p)? {
                    let ep = entry?.path();
                    if excludes.is_some_and(|s| s.contains(&ep)) {
                        continue;
                    } else if ep.is_dir() {
                        queue.push_back(ep);
                    } else {
                        result.push(ep);
                    }
                }
            }
            None => {
                break;
            }
        }
    }
    Ok(result)
}

// Checks whether a path is valid
//
// A valid path in the context of this application is the one that
//
//   1. exists
//   2. in case a symlink, is not broken and within the root dir
//
// May panic if the rootdir is a broken symlink. But since we can
// assume that rootdir is already verified before this point, it's ok
// to skip error handling for that case.
fn is_path_valid(rootdir: &Path, path: &PathBuf) -> bool {
    if path.is_symlink() {
        match path.canonicalize() {
            Ok(t) => {
                // Here we canonicalize the rootdir as well before
                // checking that the file that the symlink points to
                // is under the rootdir. This is to handle the case
                // where the rootdir itself is a symlink (For eg. on
                // MacOS, the `tmp` dir is a symlink to
                // `/private/tmp`).
                //
                // Also note that the use of `unwrap` here is
                // acceptable because at this point, it's safe to
                // assume that `rootdir` exists and is a valid file
                // path and hence, it doesn't make sense to handle
                // errors.
                let canon_rootdir = rootdir.canonicalize().unwrap();
                if fileutil::within_rootdir(&canon_rootdir, &t) {
                    true
                } else {
                    warn!("Skipping symlink to outside the root dir: {}", t.display());
                    false
                }
            }
            Err(_) => {
                warn!("Skipping broken link: {}", path.display());
                false
            }
        }
    } else {
        if path.ends_with("Icon\r") {
            warn!("Skipping Icon\\r files (macOS): {:?}", path.display());
            false
        } else {
            true
        }
    }
}

fn group_by_size(paths: Vec<&PathBuf>) -> io::Result<HashMap<u64, Vec<&PathBuf>>> {
    let mut res: HashMap<u64, Vec<&PathBuf>> = HashMap::new();
    for path in paths {
        let size = path.metadata()?.len();
        match res.get_mut(&size) {
            Some(v) => {
                v.push(path);
            }
            None => {
                res.insert(size, vec![path]);
            }
        }
    }
    Ok(res)
}

fn possible_duplicates(paths: Vec<&PathBuf>) -> io::Result<Vec<&PathBuf>> {
    let mut grps = group_by_size(paths)?;
    grps.retain(|_, v| v.len() > 1);
    let mut res: Vec<&PathBuf> = Vec::new();
    for (_, paths) in grps {
        for path in paths {
            res.push(path)
        }
    }
    Ok(res)
}

fn group_dups_by_xxh3(paths: Vec<&PathBuf>) -> io::Result<HashMap<u64, Vec<&PathBuf>>> {
    let mut res: HashMap<u64, Vec<&PathBuf>> = HashMap::new();
    for path in paths {
        let hash = fileutil::file_contents_as_xxh3_64(&path)?;
        match res.get_mut(&hash) {
            None => {
                res.insert(hash, vec![path]);
            }
            Some(v) => {
                v.push(path);
            }
        };
    }
    res.retain(|_, v| v.len() > 1);
    Ok(res)
}

fn confirm_dups(dups: HashMap<u64, Vec<&PathBuf>>) -> io::Result<HashMap<u64, Vec<&PathBuf>>> {
    let mut res: HashMap<u64, Vec<&PathBuf>> = HashMap::new();
    for (hash, paths) in dups {
        let sha256hashes = paths
            .iter()
            .map(fileutil::file_contents_as_sha256)
            .map(|x| x.unwrap())
            .collect::<HashSet<String>>();
        if sha256hashes.len() == 1 {
            res.insert(hash, paths);
        }
    }
    Ok(res)
}

fn group_duplicates<'a>(
    rootdir: &Path,
    paths: &'a Vec<PathBuf>,
    quick: &bool,
) -> io::Result<HashMap<u64, Vec<&'a PathBuf>>> {
    let valid_paths = paths
        .iter()
        .filter(|p| is_path_valid(rootdir, p))
        .collect::<Vec<&PathBuf>>();
    let poss_dups = possible_duplicates(valid_paths)?;
    let dups = group_dups_by_xxh3(poss_dups)?;
    if *quick {
        confirm_dups(dups)
    } else {
        Ok(dups)
    }
}

pub fn scan(
    rootdir: &Path,
    excludes: Option<&HashSet<PathBuf>>,
    quick: &bool,
) -> io::Result<HashMap<u64, Vec<PathBuf>>> {
    let paths = traverse_bfs(rootdir, excludes)?;
    let duplicates = group_duplicates(rootdir, &paths, quick)?
        .into_iter()
        // `group_duplicates` internally deals with PathBuf references
        // and hence returns `Vec<&PathBuf>`. So here we need to
        // create new PathBuf instances to be able to return them
        // outside the function
        .map(|(d, ps)| (d, ps.iter().map(|p| p.to_path_buf()).collect()))
        .collect::<HashMap<u64, Vec<PathBuf>>>();
    Ok(duplicates)
}
