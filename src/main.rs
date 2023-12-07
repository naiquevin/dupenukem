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

fn file_contents_as_md5<P: AsRef<Path>> (path: P) -> io::Result<Digest> {
    let data = file_contents_as_bytes(path)?;
    Ok(md5::compute(data))
}


fn scan(dirpath: &Path) -> io::Result<HashMap<Digest, Vec<Box<PathBuf>>>> {
    let mut res: HashMap<Digest, Vec<Box<PathBuf>>> = HashMap::new();
    if dirpath.is_dir() {
        for entry in fs::read_dir(dirpath)? {
            let entry = entry?;
            let hash = file_contents_as_md5(entry.path())?;
            let boxed_path = Box::new(entry.path());
            match res.get_mut(&hash) {
                None => {
                    res.insert(hash, vec![boxed_path]);
                }
                Some(v) => {
                    v.push(boxed_path);
                }
            };
        }
    }
    Ok(res)
}

fn main() {
    // println!("Hello, world!");
    let dir = Path::new("/Users/vineet/Dropbox");
    // match scan(&dir) {
    //     Ok(r) => println!("{:?}", r),
    //     Err(e) => println!("An error occurred {:?}", e),
    // };

    match traverse_bfs(&dir) {
        Ok(r) => {
            for p in r {
                println!("{:?}", p);
            }
        }
        Err(e) => println!("An error occurred {:?}", e),
    };
}
