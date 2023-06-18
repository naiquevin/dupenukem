use md5::{self, Digest};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/*

Some important points to note about the following function:

In this function we want to take a directory path, scan the contents
of it recursively and return a hashmap which will be all the paths
grouped by the md5 hash of the contents of the file.

1. Why does it return a Result instead of a HashMap? - This is because
the fs::read_dir function returns a Result<T> and we're using the `?`
operator which means if an error is returned by fs::read_dir, then
this function will also return that error

2. In the hashmap inside the result, the values are vectors of
`std::path::PathBuf` wrapped inside a Box. This is because we can't
directly use the Path object in it because Path is an unsized type
which means it can't be directly put on the stack. It has to be
allocated on the heap using Box.


*/
fn scan(dirpath: &Path) -> io::Result<HashMap<Digest, Vec<Box<PathBuf>>>> {
    let mut res: HashMap<Digest, Vec<Box<PathBuf>>> = HashMap::new();
    let path = Path::new(dirpath);
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let hash = md5::compute(entry.path().display().to_string().as_bytes());
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
    let dir = Path::new("/Users/vineet/Downloads");
    match scan(&dir) {
        Ok(r) => println!("{:?}", r),
        Err(e) => println!("An error occurred {:?}", e),
    };
}
