use crate::error::AppError;
use log::info;
use sha2::{Digest as Sha2Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use xxhash_rust::xxh3;

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

pub fn file_contents_as_xxh3_64<P: AsRef<Path>>(path: &P) -> io::Result<u64> {
    let data = file_contents_as_bytes(path)?;
    let result = xxh3::xxh3_64(&data);
    Ok(result)
}

pub fn file_contents_as_sha256<P: AsRef<Path>>(path: &P) -> io::Result<String> {
    let data = file_contents_as_bytes(path)?;
    let result = Sha256::digest(data);
    Ok(format!("{:x}", result))
}

pub fn within_rootdir(rootdir: &PathBuf, path: &PathBuf) -> bool {
    path.ancestors().find(|d| *d == rootdir).is_some()
}

/// Takes backup of the file located at `path` inside the `backup_dir`
/// directory, preserving the directory structure considering
/// 'base_dir' as the base directory for the path.
///
/// Returns Path where the file is backed up.
///
/// This function also creates backup for symlinks i.e. the file
/// content of the source path will be copied to the backups dir. This
/// is because it uses `fs::copy` function that behaves this way.
///
/// # Arguments
///
///   - path: absolute path of the file to be backed up
///   - backup_dir: directory under which the backup will be taken.
///   - base_dir: base directory using which the relative path will be
///     obtained for preserving the directory structure. Assumption is
///     that `base_dir` is an ancestor of `path`.
///
/// All paths accepted as args by this function are assumed to be
/// absolute paths.
///
/// # Errors
///
/// This function will return an error in the following situations:
///
///   - `AppError::Fs` if `base_dir` is not found to be an ancestor
///      of `path`.
///   - `AppError::Io` if there's an error writing to the backup
///      directory.
///
fn take_backup(
    path: &PathBuf,
    backup_dir: &PathBuf,
    base_dir: &PathBuf,
) -> Result<PathBuf, AppError> {
    // Find path relative to the rootdir
    let rel_path = path
        .strip_prefix(&base_dir)
        .map_err(|_| AppError::Fs(String::from("Could not find path relative to the base dir")))?;
    let backup_path = backup_dir.join(rel_path);
    fs::create_dir_all(&backup_path.parent().unwrap()).map_err(AppError::Io)?;
    fs::copy(path, &backup_path).map_err(AppError::Io)?;
    info!(
        "Backing up {} under {}",
        rel_path.display(),
        backup_dir.display()
    );
    Ok(backup_path)
}

/// Deletes a file at the given path, while optionally taking backup
///
/// Backup is optional, which is why the `backup_dir` arg is an
/// Option. Backup will be taken only if it's a `Some`.
///
/// The deletion is performed using `std::fs::remove_file`, hence it
/// works for symlinks too i.e. if `path` is a symlink, only the link
/// will be removed and the source path will not be affected.
///
/// # Errors
/// This function will return an `Err` in the following situations:
///   - If there's an error while taking backup
///   - If there is an error while deleting the file
///
pub fn delete_file(
    path: &PathBuf,
    backup_dir: Option<&PathBuf>,
    base_dir: &PathBuf,
) -> Result<(), AppError> {
    if let Some(bd) = backup_dir {
        take_backup(path, bd, base_dir)?;
    }
    fs::remove_file(path).map_err(AppError::Io)?;
    Ok(())
}

/// Replaces the file located at `path` with a symlink to
/// `source_path`, while optionally taking backup of the regular file
/// located at `path`
///
/// Backup is optional, which is why the `backup_dir` arg is an
/// Option. Backup will be taken only if it's a `Some`.
///
/// # Errors
/// This function will return an `Err` in the following situations:
///   - If there's an error while taking backup
///   - If there's an error when deleting the original file
///   - If there's an error when creating the symlink
///
pub fn replace_with_symlink(
    path: &PathBuf,
    source_path: &PathBuf,
    backup_dir: Option<&PathBuf>,
    base_dir: &PathBuf,
) -> Result<(), AppError> {
    // First delete the existing path (with backup if applicable)
    delete_file(path, backup_dir, base_dir)?;
    // Then create the symlink
    std::os::unix::fs::symlink(source_path, path).map_err(AppError::Io)
}

#[cfg(test)]
mod tests {

    use super::*;
    use serial_test::serial;
    use std::fs;
    use std::path::{Path, PathBuf};

    const TEST_DATA_DIR: &str = ".tmp-test-data";
    const TEST_FIXTURES_DIR: &str = ".tmp-test-data/fixtures";
    const TEST_BACKUP_DIR: &str = ".tmp-test-data/backups";

    fn setup() {
        let data_dir = Path::new(TEST_DATA_DIR);
        fs::remove_dir_all(data_dir).unwrap_or(());
        fs::create_dir(TEST_DATA_DIR).expect("Couldn't create TEST_DATA_DIR");
        fs::create_dir(TEST_FIXTURES_DIR).expect("Couldn't create TEST_FIXTURES_DIR");
        fs::create_dir(TEST_BACKUP_DIR).expect("Couldn't create TEST_BACKUP_DIR");
    }

    fn teardown() {
        fs::remove_dir_all(TEST_DATA_DIR).unwrap();
    }

    fn new_file<P: AsRef<Path>>(rel_path: P, contents: &str) -> PathBuf {
        let path = PathBuf::from(TEST_FIXTURES_DIR).join(rel_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn file_contents<P: AsRef<Path>>(path: P) -> String {
        fs::read_to_string(path).expect("Unable to read file")
    }

    #[test]
    #[serial]
    fn test_take_backup_ok() {
        setup();

        let f = new_file("foo.txt", "dummy data");
        let res = take_backup(
            &f,
            &PathBuf::from(TEST_BACKUP_DIR),
            &PathBuf::from(TEST_FIXTURES_DIR),
        );
        match res {
            Ok(backup_path) => {
                assert!(backup_path.is_file());
                assert_eq!("dummy data", file_contents(backup_path).as_str());
            }
            Err(_) => assert!(false),
        }

        teardown();
    }

    #[test]
    #[serial]
    fn test_take_backup_bad_base_dir() {
        setup();

        let f = new_file("foo.txt", "dummy data");
        let res = take_backup(
            &f,
            &PathBuf::from(TEST_BACKUP_DIR),
            &PathBuf::from(".non-existing-test-data-dir/fixtures"),
        );
        match res {
            Ok(_backup_path) => assert!(false),
            Err(e) => match e {
                AppError::Fs(_) => assert!(true),
                _ => assert!(false),
            },
        }

        teardown();
    }

    #[test]
    #[serial]
    fn test_take_backup_symlink() {
        setup();

        let f = new_file("foo/1.txt", "dummy file to be symlinked")
            .canonicalize()
            .expect("Couldn't canonicalize path");
        let g = PathBuf::from(TEST_FIXTURES_DIR).join("foo_1_link.txt");
        std::os::unix::fs::symlink(&f, &g).expect("Couldn't create symlink");
        assert!(g.is_symlink(), "Symlink is created");
        let res = take_backup(
            &g,
            &PathBuf::from(TEST_BACKUP_DIR),
            &PathBuf::from(TEST_FIXTURES_DIR),
        );
        match res {
            Ok(backup_path) => {
                assert!(backup_path.is_file());
                assert_eq!(
                    "dummy file to be symlinked",
                    file_contents(backup_path).as_str()
                );
            }
            Err(_) => assert!(false),
        }

        teardown();
    }

    #[test]
    #[serial]
    fn test_delete_file() {
        setup();

        let f = new_file("foo/bar/cat/1.txt", "file to be deleted");
        let backup_dir = Some(PathBuf::from(TEST_BACKUP_DIR));
        let res = delete_file(&f, backup_dir.as_ref(), &PathBuf::from(TEST_FIXTURES_DIR));
        assert!(res.is_ok(), "file deletion is successful");
        assert!(!f.try_exists().unwrap(), "file doesn't exist any more");
        let backup_path = backup_dir.unwrap().join("foo/bar/cat/1.txt");
        assert!(backup_path.is_file());
        assert_eq!("file to be deleted", file_contents(backup_path));

        teardown();
    }

    #[test]
    #[serial]
    fn test_replace_with_symlink() {
        setup();

        let path = new_file("abc/foo.txt", "file to be replaced with a symlink");
        let backup_dir = Some(PathBuf::from(TEST_BACKUP_DIR));
        let base_dir = PathBuf::from(TEST_FIXTURES_DIR);
        let src = new_file("abc/foo/main.txt", "canonical file");
        let res = replace_with_symlink(&path, &src, backup_dir.as_ref(), &base_dir);
        assert!(res.is_ok(), "replace_with_symlink returned Ok result");
        // let backup_path = backup_dir.unwrap().join("abc/foo.txt");
        // assert!(backup_path.is_file(), "original file is backed up");
        // assert!(target.is_symlink(), "file is now a soft link");
        // assert_eq!(src, target.canonicalize().unwrap(), "file is now a soft link to the src path");

        teardown();
    }
}
