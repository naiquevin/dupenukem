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

/// A "keeper" is a FilePath that's marked as 'keep'. There's a global
/// assumption in this app that in a valid snapshot, every group (of
/// duplicates) must have at least 1 path marked as 'keep'
fn find_keeper(filepaths: &Vec<FilePath>) -> Option<&FilePath> {
    filepaths
        .iter()
        .find(|filepath| filepath.op == FileOp::Keep)
}

fn validate_group(hash: &Digest, filepaths: &Vec<FilePath>) -> Result<(), Error> {
    let n = filepaths.len();
    if n <= 1 {
        return Err(Error::CorruptSnapshot(format!(
            "Group must contain at least 2 paths; {} found for {:x}",
            n, hash
        )));
    }

    match find_keeper(filepaths) {
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

fn partially_validate_path_to_symlink<'a>(
    filepath: &'a FilePath,
    source: Option<&PathBuf>,
    _default_source: &PathBuf,
) -> Result<Action<'a>, Error> {
    let path = &filepath.path;
    if path.is_symlink() {
        // Path is a symlink but the action to take depends on whether
        // it can be resolved or not (broken).
        match path.canonicalize() {
            // If the symlink is valid, we further check whether the
            // source path it resolves to matches the source (if
            // provided). If yes, it's a no-op. If not, it's an error
            // (operation not allowed)
            Ok(src_path) => {
                // @TODO: The case where source is None needs to be
                // handled by falling back to default source
                if source.is_none() || source.is_some_and(|p| *p == src_path) {
                    Ok(Action {
                        filepath,
                        is_no_op: true,
                    })
                } else {
                    Err(Error::OpNotAllowed(format!(
                        "Specified symlink source path {} doesn't match the actual source path {}",
                        // Use of `unwrap` is acceptable here because
                        // the case of `source` being None is handled
                        // in the if clause.
                        source.unwrap().display(),
                        src_path.display(),
                    )))
                }
            }
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

fn validate_path<'a>(
    rootdir: &PathBuf,
    hash: &Digest,
    filepath: &'a FilePath,
    keeper: &FilePath,
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

    let action = match &filepath.op {
        FileOp::Keep => partially_validate_path_to_keep(filepath)?,
        FileOp::Symlink { source } => {
            partially_validate_path_to_symlink(filepath, source.as_ref(), &keeper.path)?
        }
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

pub fn validate(snap: &Snapshot) -> Result<Vec<Action>, Error> {
    let mut actions: Vec<Action> = Vec::new();
    if let Err(e) = validate_rootdir(&snap.rootdir) {
        return Err(e);
    }

    for (hash, filepaths) in snap.duplicates.iter() {
        if let Err(e) = validate_group(hash, filepaths) {
            return Err(e);
        }

        let keeper = find_keeper(filepaths).unwrap();

        for filepath in filepaths.iter() {
            match validate_path(&snap.rootdir, hash, filepath, keeper) {
                Ok(action) => actions.push(action),
                Err(e) => return Err(e),
            }
        }
    }

    Ok(actions)
}
