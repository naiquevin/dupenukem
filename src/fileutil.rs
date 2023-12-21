use log::{debug, warn};
use md5::{self, Digest};
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

pub fn within_rootdir(rootdir: &Path, path: &PathBuf) -> bool {
    path.ancestors().find(|d| *d == rootdir).is_some()
}

fn try_md5_hash(rootdir: &Path, path: &PathBuf) -> Option<Digest> {
    if path.is_symlink() {
        if within_rootdir(rootdir, path) {
            match path.canonicalize().ok() {
                Some(t) => {
                    debug!("Reading file: {} -> {}", path.display(), t.display());
                    file_contents_as_md5(&t).ok()
                }
                None => {
                    warn!("Skipping broken link: {}", path.display());
                    None
                }
            }
        } else {
            warn!(
                "Skipping symlink to outside the root dir: {}",
                path.display()
            );
            None
        }
    } else {
        debug!("Reading file: {}", path.display());
        file_contents_as_md5(&path).ok()
    }
}

pub fn find_duplicates<'a>(
    rootdir: &Path,
    paths: &'a Vec<PathBuf>,
) -> io::Result<HashMap<Digest, Vec<&'a PathBuf>>> {
    let mut res: HashMap<Digest, Vec<&PathBuf>> = HashMap::new();
    for path in paths {
        if let Some(hash) = try_md5_hash(rootdir, path) {
            match res.get_mut(&hash) {
                None => {
                    res.insert(hash, vec![path]);
                }
                Some(v) => {
                    v.push(path);
                }
            };
        }
    }
    res.retain(|_, v| v.len() > 1);
    Ok(res)
}
