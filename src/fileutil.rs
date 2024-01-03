use md5::{self, Digest};
use sha2::{Digest as Sha2Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

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
