use super::{FileOp, FilePath, Snapshot};
use crate::executor::Action;
use crate::fileutil;
use crate::hash::Checksum;
use log::warn;
use std::io;
use std::path::{Path, PathBuf};

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

fn validate_rootdir(path: &Path) -> Result<(), Error> {
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
/// duplicates) must have at least 1 path marked as 'keep'. This
/// function returns the first occurrence of FilePath marked 'keep'.
fn find_keeper(filepaths: &Vec<FilePath>) -> Option<&FilePath> {
    filepaths
        .iter()
        .find(|filepath| filepath.op == FileOp::Keep)
}

fn validate_group(hash: &Checksum, filepaths: &Vec<FilePath>) -> Result<(), Error> {
    let n = filepaths.len();
    if n <= 1 {
        return Err(Error::CorruptSnapshot(format!(
            "Group must contain at least 2 paths; {n} found for {hash}"
        )));
    }

    match find_keeper(filepaths) {
        Some(_) => Ok(()),
        None => Err(Error::OpNotAllowed(format!(
            "Group must contain at least 1 path marked 'keep'. None found for {hash}"
        ))),
    }
}

fn validate_checksum(path: &PathBuf, expected_hash: &Checksum) -> Result<(), Error> {
    let computed_hash = Checksum::of_file(path).map_err(Error::Io)?;
    if computed_hash == *expected_hash {
        Ok(())
    } else {
        Err(Error::ChecksumMismatch {
            path: path.display().to_string(),
            actual: format!("{}", computed_hash),
            expected: format!("{}", expected_hash),
        })
    }
}

fn validate_path_to_keep<'a>(
    filepath: &'a FilePath,
    expected_hash: &Checksum,
) -> Result<Action<'a>, Error> {
    let path = &filepath.path;
    if path.is_symlink() {
        // Path is a symlink (doesn't matter if it's broken)
        Err(Error::OpNotPossible(format!(
            "Operation 'keep' not possible on a symlink: {}",
            path.display()
        )))
    } else if path.is_file() {
        // Path is a regular file
        validate_checksum(&filepath.path, expected_hash)?;
        Ok(Action::Keep(&filepath.path))
    } else {
        // Path doesn't exist
        Err(Error::OpNotPossible(format!(
            "Operation 'keep' not possible on non-existing path: {}",
            path.display()
        )))
    }
}

/// Verifies the hash of the symlink source file by comparing it with
/// the hash of the target.
///
/// Instead of computing the hash of the `target` file for comparison,
/// it accepts an already computed `hash` as the 3rd argument.
///
/// The `target` argument is required to resolve the source path in
/// relation to the target, in case it's a relative path.
///
/// # Errors
///
/// This function returns `Err` in following situations:
///   - if the absolute source path cannot be resolved (in relation to
///     the target)
///   - if the hash of the source file contents cannot be obtained for
///     any reason.
///
fn verify_symlink_source_hash(
    source: &Path,
    target: &Path,
    target_hash: &Checksum,
) -> Result<bool, Error> {
    let src_hash = if source.is_absolute() {
        Checksum::of_file(&source).map_err(Error::Io)
    } else {
        let p = target
            .parent()
            .unwrap()
            .join(source)
            .canonicalize()
            .map_err(Error::Io)?;
        Checksum::of_file(&p).map_err(Error::Io)
    }?;
    Ok(src_hash == *target_hash)
}

/// Verifies if actual source path and intended source path are the same.
///
/// This function is relevant only in the case where the file is
/// marked "symlink" and is already a symlink. In this case, we need
/// to make sure that both are the same before nooping.
fn verify_symlink_source_path(
    intended_source: &Path,
    actual_source: &Path,
    target: &Path,
    is_explicit: bool,
) -> Result<bool, Error> {
    let is_intended_abs = intended_source.is_absolute();
    let is_actual_abs = actual_source.is_absolute();
    if is_explicit || (is_intended_abs && is_actual_abs) {
        // Compare the paths directly if,
        //   - intended is explicitly specified, or
        //   - both are absolute
        Ok(*intended_source == *actual_source)
    } else if is_intended_abs && !is_actual_abs {
        // If intended is absolute but actual is relative, then
        // convert actual to absolute and compare
        let p = target
            .parent()
            .unwrap()
            .join(actual_source)
            .canonicalize()
            .map_err(Error::Io)?;
        Ok(*intended_source == *p)
    } else {
        // The remaining case is - intended is relative (whereas
        // actual may or may not be relative). Unless there's a bug,
        // this case is not expected to occur
        Err(Error::OpNotAllowed(format!(
            "Implicit intended source path cannot be relative"
        )))
    }
}

fn validate_path_to_symlink<'a>(
    filepath: &'a FilePath,
    source: Option<&'a PathBuf>,
    default_source: &'a PathBuf,
    expected_hash: &Checksum,
) -> Result<Action<'a>, Error> {
    let path = &filepath.path;

    // Validate checksum of the file against the expected value
    validate_checksum(path, expected_hash)?;

    // If source path is `Some` which means it's specified by the
    // user, verify that it's hash matches that of the group. This is
    // to prevent the user from specifying some other file as the
    // symlink source path (a common copy-paste mistake).
    if let Some(src) = source {
        if !verify_symlink_source_hash(src, &filepath.path, expected_hash)? {
            return Err(Error::OpNotPossible(format!(
                "Hash mismatch for specified symlink source path: {} -> {}",
                filepath.path.display(),
                src.display()
            )));
        }
    }

    let intended_src_path = source.unwrap_or(default_source);

    // If the intended source path is itself a symlink, it's not
    // supported/allowed. Note that this check is important regardless
    // of whether the source is specified by the user.
    if intended_src_path.is_symlink() {
        return Err(Error::OpNotAllowed(format!(
            "Source path cannot be a symlink itself: {}",
            intended_src_path.display()
        )));
    }

    let is_explicit = source.is_some();

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
                if verify_symlink_source_path(
                    intended_src_path,
                    &actual_src_path,
                    path,
                    is_explicit,
                )? {
                    Ok(Action::Symlink {
                        path: &filepath.path,
                        source: intended_src_path,
                        is_explicit,
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
                is_explicit,
                is_no_op: false,
            }),
        }
    } else if filepath.path.is_file() {
        Ok(Action::Symlink {
            path: &filepath.path,
            source: intended_src_path,
            is_explicit,
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

fn validate_path_to_delete<'a>(
    filepath: &'a FilePath,
    expected_hash: &Checksum,
) -> Result<Action<'a>, Error> {
    let path = &filepath.path;
    if path.exists() {
        match path.canonicalize() {
            Ok(_) => {
                // Verify that the hash matches
                validate_checksum(path, expected_hash)?;
                Ok(Action::Delete {
                    path: &path,
                    is_no_op: false,
                })
            }
            Err(_) => Err(Error::OpNotAllowed(format!(
                "Couldn't verify file marked for deletion: {}",
                path.display()
            ))),
        }
    } else {
        warn!("Already deleted file will be ignored: {}", path.display());
        Ok(Action::Delete {
            path: &path,
            is_no_op: true,
        })
    }
}

fn validate_path<'a>(
    rootdir: &Path,
    hash: &Checksum,
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
        FileOp::Keep => validate_path_to_keep(filepath, hash)?,
        FileOp::Symlink { source } => {
            validate_path_to_symlink(filepath, source.as_ref(), &keeper.path, hash)?
        }
        FileOp::Delete => validate_path_to_delete(filepath, hash)?,
    };

    Ok(action)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;

    #[test]
    fn test_verify_symlink_source_path_parallel() {
        let t = PathBuf::from("/private/tmp/bar/current");
        // Cases:
        //
        // is_explicit           : true|false
        // intended source type  : abs|rel
        // actual source type    : abs|rel

        // (true, abs, abs)
        let i = PathBuf::from("/private/tmp/foo/1.txt");
        // when both paths match
        let a = PathBuf::from("/private/tmp/foo/1.txt");
        match verify_symlink_source_path(&i, &a, &t, true) {
            Ok(b) => assert!(b),
            Err(_) => assert!(false),
        }
        // when paths don't match
        let b = PathBuf::from("/private/tmp/bar/1.txt");
        match verify_symlink_source_path(&i, &b, &t, true) {
            Ok(b) => assert!(!b),
            Err(_) => assert!(false),
        }

        // (true, abs, rel)
        let i = PathBuf::from("/private/tmp/foo/1.txt");
        let a = PathBuf::from("../foo/1.txt");
        match verify_symlink_source_path(&i, &a, &t, true) {
            Ok(b) => assert!(!b),
            Err(_) => assert!(false),
        }

        // (true, rel, rel)
        let i = PathBuf::from("../foo/1.txt");
        // when both paths match
        let a = PathBuf::from("../foo/1.txt");
        match verify_symlink_source_path(&i, &a, &t, true) {
            Ok(b) => assert!(b),
            Err(_) => assert!(false),
        }
        // when paths don't match
        let b = PathBuf::from("../bar/1.txt");
        match verify_symlink_source_path(&i, &b, &t, true) {
            Ok(b) => assert!(!b),
            Err(_) => assert!(false),
        }

        // (true, rel, abs)
        let i = PathBuf::from("../foo/1.txt");
        let a = PathBuf::from("foo/1.txt");
        match verify_symlink_source_path(&i, &a, &t, true) {
            Ok(b) => assert!(!b),
            Err(_) => assert!(false),
        }

        // (false, abs, abs)
        let i = PathBuf::from("/private/tmp/foo/1.txt");
        // when both paths match
        let a = PathBuf::from("/private/tmp/foo/1.txt");
        match verify_symlink_source_path(&i, &a, &t, false) {
            Ok(b) => assert!(b),
            Err(_) => assert!(false),
        }
        // when paths don't match
        let b = PathBuf::from("/private/tmp/bar/1.txt");
        match verify_symlink_source_path(&i, &b, &t, false) {
            Ok(b) => assert!(!b),
            Err(_) => assert!(false),
        }

        // (false, abs, rel) <- This test case needs some files to be
        // actually created. Hence it's added as a separate test case
        // `test_verify_symlink_source_path_serial` which is
        // configured to run serially.

        // (false, rel, rel) <- exceptional case not expected unless
        // there's a bug
        let i = PathBuf::from("../foo/1.txt");
        let a = PathBuf::from("../foo/1.txt");
        assert!(verify_symlink_source_path(&i, &a, &t, false).is_err());

        // (false, rel, abs) <- exceptional case not expected unless
        // there's a bug
        let i = PathBuf::from("../foo/1.txt");
        let a = PathBuf::from("/tmp/foo/1.txt");
        assert!(verify_symlink_source_path(&i, &a, &t, false).is_err());
    }

    #[test]
    #[serial]
    fn test_verify_symlink_source_path_serial() {
        let test_data_dir = Path::new(".tmp-test-data");
        // cleanup old test dir in case required
        fs::remove_dir_all(".tmp-test-data").unwrap_or(());
        // create test dir
        fs::create_dir(test_data_dir).expect("Couldn't create test data dir");
        let test_data_dir_abs = test_data_dir.canonicalize().unwrap();

        // Create target dir (no need to create target file)
        let target = test_data_dir_abs.join("bar/current");
        fs::create_dir(target.parent().unwrap()).unwrap();

        // Create a file (which will be the actual symlink source
        // path)
        let actual = test_data_dir.join("foo/1.txt");
        fs::create_dir(actual.parent().unwrap()).unwrap();
        fs::write(&actual, "Foo 1").unwrap();
        let actual_abs = actual.canonicalize().unwrap();

        // Create a file (incorrect symlink source path to test the
        // mismatch case)
        let incorrect = test_data_dir.join("cat/1.txt");
        fs::create_dir(incorrect.parent().unwrap()).unwrap();
        fs::write(&incorrect, "Cat 1").unwrap();

        let i = actual_abs;
        // when both paths match
        let a = PathBuf::from("../foo/1.txt");
        match verify_symlink_source_path(&i, &a, &target, false) {
            Ok(b) => assert!(b),
            Err(_e) => assert!(false),
        }
        // when paths don't match
        let b = PathBuf::from("../cat/1.txt");
        match verify_symlink_source_path(&i, &b, &target, false) {
            Ok(b) => assert!(!b),
            Err(_) => assert!(false),
        }

        // teardown
        fs::remove_dir_all(".tmp-test-data").unwrap();
    }
}
