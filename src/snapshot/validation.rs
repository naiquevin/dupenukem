use super::{FileOp, FilePath, Snapshot};
use crate::executor::Action;
use crate::fileutil;
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

fn validate_group(hash: &u64, filepaths: &Vec<FilePath>) -> Result<(), Error> {
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
        Ok(Action::Keep(&filepath.path))
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
    source: Option<&'a PathBuf>,
    default_source: &'a PathBuf,
) -> Result<Action<'a>, Error> {
    let path = &filepath.path;
    let intended_src_path = source.unwrap_or(default_source);

    // If the intended source path is itself a symlink, it's not
    // supported/allowed
    if intended_src_path.is_symlink() {
        return Err(Error::OpNotAllowed(format!(
            "Source path cannot be a symlink itself: {}",
            intended_src_path.display()
        )));
    }

    // Here we also derive whether the source path should be relative
    // or absolute. If it's specified by the user, consider that else
    // assume relative.
    let is_sym_relative = match source {
        Some(p) => p.is_relative(),
        None => true,
    };

    if path.is_symlink() {
        // Path is a symlink but the action to take depends on whether
        // it can be resolved or not (broken). @Note that we're using
        // `read_link` instead of `canonicalize` as the latter will
        // also perform an implicit conversion to absolute path.
        match path.read_link() {
            // If the symlink is valid, we further check whether the
            // source path it resolves to matches the intended source
            // path derived above. If yes, it's a no-op. Otherwise,
            // it's an error (operation not allowed)
            Ok(actual_src_path) => {
                if *intended_src_path == actual_src_path {
                    Ok(Action::Symlink {
                        path: &filepath.path,
                        source: intended_src_path,
                        is_relative: is_sym_relative,
                        is_no_op: true,
                    })
                } else {
                    Err(Error::OpNotAllowed(format!(
                        "Updation of symlink source path is not supported: {}",
                        path.display(),
                    )))
                }
            }
            // If it's a broken symlink, it can just be fixed
            Err(_) => Ok(Action::Symlink {
                path: &filepath.path,
                source: intended_src_path,
                is_relative: is_sym_relative,
                is_no_op: false,
            }),
        }
    } else if filepath.path.is_file() {
        Ok(Action::Symlink {
            path: &filepath.path,
            source: intended_src_path,
            is_relative: is_sym_relative,
            is_no_op: false,
        })
    } else {
        // Path doesn't exist. This basically means that the tool can
        // be used to only replace existing files with symlinks
        // i.e. it can't be used for creating new symlinks
        Err(Error::OpNotPossible(format!(
            "Operation 'symlink' not possible for non-existing path: {} ",
            filepath.path.display()
        )))
    }
}

fn partially_validate_path_to_delete<'a>(filepath: &'a FilePath) -> Result<Action<'a>, Error> {
    let path = &filepath.path;
    // Check if the path exists and can be resolved if it's a symlink
    match path.canonicalize() {
        Ok(_) => Ok(Action::Delete {
            path: &filepath.path,
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
    hash: &u64,
    filepath: &'a FilePath,
    keeper: &'a FilePath,
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

    let computed_hash = fileutil::file_contents_as_xxh3_64(&filepath.path).map_err(Error::Io)?;

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

        // As the call to `validate_group` must have validated that
        // there's at least one 'keep' entry, there's no need to
        // handle None value.
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
