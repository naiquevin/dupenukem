use crate::error::AppError;
use crate::fileutil::file_contents_as_bytes;
use sha2::{Digest, Sha256};
use std::fmt;
use std::io;
use std::path::Path;
use xxhash_rust::xxh3;

pub fn xxh3_64<P: AsRef<Path>>(path: &P) -> io::Result<u64> {
    let data = file_contents_as_bytes(path)?;
    let result = xxh3::xxh3_64(&data);
    Ok(result)
}

pub fn sha256<P: AsRef<Path>>(path: &P) -> io::Result<String> {
    let data = file_contents_as_bytes(path)?;
    let result = Sha256::digest(data);
    Ok(format!("{result:x}"))
}

/// Wrapper around xx3_64 hash
///
/// The intention is to be able to swap out the checksum/hashing
/// algorithm in future without having to modify the calling code.
#[derive(PartialEq, Eq, Hash)]
pub struct Checksum {
    xx3_hash: u64,
}

impl Checksum {
    pub fn new(value: u64) -> Self {
        Self { xx3_hash: value }
    }

    pub fn of_file<P: AsRef<Path>>(path: &P) -> io::Result<Self> {
        let hash = xxh3_64(path)?;
        Ok(Self { xx3_hash: hash })
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        let hash = s.parse::<u64>().map_err(|_| AppError::ChecksumParsing)?;
        Ok(Self { xx3_hash: hash })
    }

    // Returns the actual hash value
    //
    // @NOTE: In case the hashing algorithm gets changed in future,
    // this function should return a value that uniquely identifies
    // the checksum and implements the `Copy` Trait. This way, we can
    // ensure that there's no hard requirement for the struct to
    // implement the Copy trait.
    pub fn value(&self) -> u64 {
        self.xx3_hash
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.xx3_hash)
    }
}
