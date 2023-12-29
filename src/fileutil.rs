use log::warn;
use md5::{self, Digest};
use sha2::{Digest as Sha2Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Traverses the `dirpath` recursively using breadth first search
/// approach and returns a vector of `PathBuf`.
///
/// Optionally, a hashset of `PathBuf` refs can be passed as the
/// `excludes` arg. These paths will be excluded during traversal.
pub fn traverse_bfs(
    dirpath: &Path,
    excludes: Option<&HashSet<PathBuf>>,
) -> io::Result<Vec<PathBuf>> {
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

/*

In the following 2 functions, the argument is defined using generics
as AsRef<Path>. This basically means that the argument can be of any
type that implements the trait AsRef<Path>.

The most common usecase is to accept Path, PathBuf or sometimes even
strings. When working with Path types, the methods we usually
encounter return either &Path (reference to the data) or PathBuf (copy
that actually holds the data). So it helps if any functions that we
implement also support the same generics.

*/

fn file_contents_as_bytes<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let mut buf: Vec<u8> = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn file_contents_as_md5<P: AsRef<Path>>(path: &P) -> io::Result<Digest> {
    let data = file_contents_as_bytes(path)?;
    Ok(md5::compute(data))
}

pub fn file_contents_as_sha256<P: AsRef<Path>>(path: &P) -> io::Result<String> {
    let data = file_contents_as_bytes(path)?;
    let result = Sha256::digest(data);
    Ok(format!("{:x}", result))
}

pub fn within_rootdir(rootdir: &PathBuf, path: &PathBuf) -> bool {
    path.ancestors().find(|d| *d == rootdir).is_some()
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
                if within_rootdir(&canon_rootdir, &t) {
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

fn group_dups_by_md5(paths: Vec<&PathBuf>) -> io::Result<HashMap<Digest, Vec<&PathBuf>>> {
    let mut res: HashMap<Digest, Vec<&PathBuf>> = HashMap::new();
    for path in paths {
        let hash = file_contents_as_md5(&path)?;
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

fn confirm_dups(
    dups: HashMap<Digest, Vec<&PathBuf>>,
) -> io::Result<HashMap<Digest, Vec<&PathBuf>>> {
    let mut res: HashMap<Digest, Vec<&PathBuf>> = HashMap::new();
    for (md5hash, paths) in dups {
        let sha256hashes = paths
            .iter()
            .map(file_contents_as_sha256)
            .map(|x| x.unwrap())
            .collect::<HashSet<String>>();
        if sha256hashes.len() == 1 {
            res.insert(md5hash, paths);
        }
    }
    Ok(res)
}

pub fn find_duplicates<'a>(
    rootdir: &Path,
    paths: &'a Vec<PathBuf>,
    quick: &bool,
) -> io::Result<HashMap<Digest, Vec<&'a PathBuf>>> {
    let valid_paths = paths
        .iter()
        .filter(|p| is_path_valid(rootdir, p))
        .collect::<Vec<&PathBuf>>();
    let poss_dups = possible_duplicates(valid_paths)?;
    let dups = group_dups_by_md5(poss_dups)?;
    if *quick {
        confirm_dups(dups)
    } else {
        Ok(dups)
    }
}
