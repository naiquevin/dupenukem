use super::{Action, FileOp, FilePath, Snapshot};
use crate::fileutil;
use md5::Digest;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    RootDir(String),
    OpNotPossible(String),
    OpNotAllowed(String),
    CorruptSnapshot(String),
    ChecksumMismatch {
        path: String,
        actual: String,
        expected: String,
    },
    Io(io::Error),
}

#[allow(dead_code)]
fn validate_rootdir(path: &PathBuf) -> Result<(), Error> {
    match path.try_exists() {
        Ok(true) => Ok(()),
        Ok(false) => Err(Error::RootDir(format!(
            "The rootdir {} doesn't exist",
            path.display()
        ))),
        Err(_) => Err(Error::RootDir(format!(
            "Failed to check rootdir {}",
            path.display()
        ))),
    }
}

#[allow(dead_code)]
fn validate_group(hash: &Digest, filepaths: &Vec<FilePath>) -> Result<(), Error> {
    let n = filepaths.len();
    if n <= 1 {
        return Err(Error::CorruptSnapshot(format!(
            "Group must contain at least 2 paths; {} found for {:x}",
            n, hash
        )));
    }

    match filepaths
        .iter()
        .find(|filepath| filepath.op == FileOp::Keep)
    {
        Some(_) => Ok(()),
        None => Err(Error::OpNotAllowed(format!(
            "Group must contain at least 1 path marked 'keep'. None found for {:x}",
            hash
        ))),
    }
}

fn partially_validate_path_to_keep(filepath: &FilePath) -> Result<Action, Error> {
    let path = &filepath.path;
    if path.is_symlink() {
        // Path is a symlink (doesn't matter if it's broken)
        Err(Error::OpNotPossible(format!(
            "Operation 'keep' not possible on a symlink: {}",
            path.display()
        )))
    } else if path.is_file() {
        // Path is a regular file
        Ok(Action {
            filepath,
            is_no_op: true,
        })
    } else {
        // Path doesn't exist
        Err(Error::OpNotPossible(format!(
            "Operation 'keep' not possible on non-existing path: {}",
            path.display()
        )))
    }
}

fn partially_validate_path_to_symlink<'a>(filepath: &'a FilePath) -> Result<Action<'a>, Error> {
    let path = &filepath.path;
    if path.is_symlink() {
        // Path is a symlink but the action to take depends on whether
        // it can be resolved or not (broken).
        match path.canonicalize() {
            // If the symlink is valid, it's a no-op
            Ok(_) => Ok(Action {
                filepath,
                is_no_op: true,
            }),
            // If it's a broken symlink, it can just be fixed
            Err(_) => Ok(Action {
                filepath,
                is_no_op: false,
            }),
        }
    } else if filepath.path.is_file() {
        // Path is a regular file
        Ok(Action {
            filepath,
            is_no_op: false,
        })
    } else {
        // Path doesn't exist
        Err(Error::OpNotPossible(format!(
            "Operation 'symlink' not possible on non-existing path: {} ",
            filepath.path.display()
        )))
    }
}

fn partially_validate_path_to_delete<'a>(filepath: &'a FilePath) -> Result<Action<'a>, Error> {
    let path = &filepath.path;
    // Check if the path exists and can be resolved if it's a symlink
    match path.canonicalize() {
        Ok(_) => Ok(Action {
            filepath,
            is_no_op: false,
        }),
        Err(_) => Err(Error::OpNotAllowed(format!(
            "Couldn't verify file marked for deletion: {}",
            path.display()
        ))),
    }
}

#[allow(dead_code)]
fn validate_path<'a>(
    rootdir: &PathBuf,
    hash: &Digest,
    filepath: &'a FilePath,
) -> Result<Action<'a>, Error> {
    let path = &filepath.path;

    // If the path is external to the rootdir, return an error right
    // away
    if !fileutil::within_rootdir(rootdir, &path) {
        return Err(Error::CorruptSnapshot(format!(
            "Path {} is external to the rootdir",
            path.display()
        )));
    }

    let action = match filepath.op {
        FileOp::Keep => partially_validate_path_to_keep(filepath)?,
        FileOp::Symlink => partially_validate_path_to_symlink(filepath)?,
        FileOp::Delete => partially_validate_path_to_delete(filepath)?,
    };

    let computed_hash = fileutil::file_contents_as_md5(&action.filepath.path).map_err(Error::Io)?;

    if computed_hash == *hash {
        Ok(action)
    } else {
        Err(Error::ChecksumMismatch {
            path: path.display().to_string(),
            actual: format!("{:x}", computed_hash),
            expected: format!("{:x}", hash),
        })
    }
}

#[allow(dead_code)]
fn validate(snap: &Snapshot) -> Result<Vec<Action>, Error> {
    let mut actions: Vec<Action> = Vec::new();
    if let Err(e) = validate_rootdir(&snap.rootdir) {
        return Err(e);
    }

    for (hash, filepaths) in snap.duplicates.iter() {
        if let Err(e) = validate_group(hash, filepaths) {
            return Err(e);
        }

        for filepath in filepaths.iter() {
            match validate_path(&snap.rootdir, hash, filepath) {
                Ok(action) => actions.push(action),
                Err(e) => return Err(e),
            }
        }
    }

    Ok(actions)
}
