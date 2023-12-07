use md5::{self, Digest};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};


/// Function to traverse a directory recursively using the
/// breadth-first approach and return a vector of paths to all the
/// files.
fn traverse_bfs(dirpath: &Path) -> io::Result<Vec<PathBuf>> {
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    let mut result: Vec<PathBuf> = Vec::new();
    queue.push_back(dirpath.to_path_buf());
    loop {
        match queue.pop_front() {
            Some(p) => {
                for entry in fs::read_dir(p)? {
                    let ep = entry?.path();
                    if ep.is_dir() {
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

fn file_contents_as_md5<P: AsRef<Path>> (path: &P) -> io::Result<Digest> {
    let data = file_contents_as_bytes(path)?;
    Ok(md5::compute(data))
}


fn find_duplicates(paths: &Vec<PathBuf>) -> io::Result<HashMap<Digest, Vec<&PathBuf>>> {
    let mut res: HashMap<Digest, Vec<&PathBuf>> = HashMap::new();
    for path in paths {
        // @TODO: For now, all symlinks are being ignored. Actually we
        // want to consider those symlinks that are under the root
        // directory
        if !path.is_symlink() {
            println!("Reading file: {}", path.display());
            let hash = file_contents_as_md5(&path)?;
            match res.get_mut(&hash) {
                None => {
                    res.insert(hash, vec![path]);
                }
                Some(v) => {
                    v.push(path);
                }
            };
        } else {
            println!("Skipping symlink: {}", path.display());
        }
    }
    Ok(res)
}

fn main() {
    let dir = Path::new("/Users/vineet/Dropbox");
    let paths = traverse_bfs(&dir).unwrap();
    let dups = find_duplicates(&paths).unwrap();
    println!("Duplicates: {:?}", dups);
}
