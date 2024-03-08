use crate::error::AppError;
use crate::executor::Action;
use crate::hash::Checksum;
use crate::scanner::scan;
use chrono::{DateTime, FixedOffset, Local};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

pub mod textformat;
pub mod validation;

#[derive(Debug, PartialEq, Eq, Clone, PartialOrd, Ord)]
enum FileOp {
    Keep,
    Symlink {
        // The `PathBuf` may be absolute or relative. We should never
        // canonicalize it so that the type of symlink that gets
        // created (relative or asbsolute) is exactly as per how the
        // user has specified it in the input snapshot file.
        source: Option<PathBuf>,
    },
    Delete,
}

impl FileOp {
    fn decode(keyword: &str, extra: Option<&str>) -> Option<Self> {
        match keyword {
            "keep" => Some(Self::Keep),
            "symlink" => Some(Self::Symlink {
                source: extra.map(PathBuf::from),
            }),
            "delete" => Some(Self::Delete),
            // @TODO: Throw an error here
            _ => None,
        }
    }

    fn keyword(&self) -> &str {
        match self {
            Self::Keep => "keep",
            Self::Symlink { source: _ } => "symlink",
            Self::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilePath {
    path: PathBuf,
    op: FileOp,
}

impl FilePath {
    fn new(path: PathBuf) -> FilePath {
        let op = if path.is_symlink() {
            // @NOTE: Here we're not handling the case where
            // `canonicalize` returns an Err
            FileOp::Symlink {
                source: path.read_link().ok(),
            }
        } else {
            FileOp::Keep
        };
        FilePath { path, op }
    }

    fn size(&self) -> io::Result<u64> {
        let metadata = self.path.metadata()?;
        Ok(metadata.len())
    }
}

/// Returns "keeper" of the duplicate group
///
/// A "keeper" is a FilePath that's marked as 'keep'. There's a global
/// assumption in this app that in a valid snapshot, every group (of
/// duplicates) must have at least 1 path marked as 'keep'. This
/// function sorts the filepaths and returns the first occurrence
/// that's marked 'keep'. Sorting increases the chance of the same
/// path being considered the keeper, which helps in matching implicit
/// symlink source paths during validation.
fn find_keeper(filepaths: &[FilePath]) -> Option<&FilePath> {
    let mut filepaths_sorted = filepaths.to_vec();
    filepaths_sorted.sort();
    filepaths_sorted
        .iter()
        .find(|filepath| filepath.op == FileOp::Keep)
        .and_then(|k| filepaths.iter().find(|fp| fp.path == k.path))
}

/// Checks whether all filepaths in a duplicate group are marked for
/// deletion
fn are_all_deletions(filepaths: &[FilePath]) -> bool {
    filepaths
        .iter()
        .all(|filepath| filepath.op == FileOp::Delete)
}

/// Returns if the group is already de-duped by checking whether there
/// is only one path marked Keep and the rest marked Symlink
fn is_group_deduped(filepaths: &[FilePath]) -> bool {
    let mut num_keeps = 0;
    for filepath in filepaths {
        match filepath.op {
            FileOp::Keep => num_keeps += 1,
            FileOp::Delete => return false,
            FileOp::Symlink { source: _ } => {}
        }
    }
    num_keeps == 1
}

pub struct Snapshot {
    pub rootdir: PathBuf,
    generated_at: DateTime<FixedOffset>,
    duplicates: HashMap<Checksum, Vec<FilePath>>,
}

impl Snapshot {
    pub fn of_rootdir(
        rootdir: &Path,
        excludes: Option<&HashSet<PathBuf>>,
        quick: &bool,
        skip_deduped: &bool,
    ) -> io::Result<Snapshot> {
        let duplicates = scan(rootdir, excludes, quick)?
            .into_iter()
            .map(|(checksum, paths)| {
                (
                    checksum,
                    paths
                        .into_iter()
                        .map(FilePath::new)
                        .collect::<Vec<FilePath>>(),
                )
            })
            .filter(|(_, group)| !(*skip_deduped && is_group_deduped(group)))
            .collect::<HashMap<Checksum, Vec<FilePath>>>();
        let snap = Snapshot {
            rootdir: rootdir.to_path_buf(),
            generated_at: Local::now().fixed_offset(),
            duplicates,
        };
        Ok(snap)
    }

    pub fn validate(&self, is_full_deletion_allowed: &bool) -> Result<Vec<Action>, AppError> {
        validation::validate(self, is_full_deletion_allowed).map_err(AppError::SnapshotValidation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_keeper() {
        let fps = vec![
            FilePath {
                path: PathBuf::from("d.txt"),
                op: FileOp::Keep,
            },
            FilePath {
                path: PathBuf::from("a.txt"),
                op: FileOp::Delete,
            },
            FilePath {
                path: PathBuf::from("b.txt"),
                op: FileOp::Keep,
            },
            FilePath {
                path: PathBuf::from("c.txt"),
                op: FileOp::Keep,
            },
            FilePath {
                path: PathBuf::from("e.txt"),
                op: FileOp::Delete,
            },
        ];
        assert_eq!(Some(&fps[2]), find_keeper(&fps));

        let fps = vec![
            FilePath {
                path: PathBuf::from("d.txt"),
                op: FileOp::Delete,
            },
            FilePath {
                path: PathBuf::from("a.txt"),
                op: FileOp::Delete,
            },
        ];
        assert!(find_keeper(&fps).is_none());
    }

    #[test]
    fn test_is_group_deduped() {
        let g = vec![
            FilePath {
                path: PathBuf::from("/foo/1.txt"),
                op: FileOp::Keep,
            },
            FilePath {
                path: PathBuf::from("/bar/1.txt"),
                op: FileOp::Symlink { source: None },
            },
            FilePath {
                path: PathBuf::from("/cat/1.txt"),
                op: FileOp::Symlink { source: None },
            },
        ];
        assert!(is_group_deduped(&g));

        let g = vec![
            FilePath {
                path: PathBuf::from("/foo/1.txt"),
                op: FileOp::Keep,
            },
            FilePath {
                path: PathBuf::from("/bar/1.txt"),
                op: FileOp::Keep,
            },
            FilePath {
                path: PathBuf::from("/cat/1.txt"),
                op: FileOp::Symlink { source: None },
            },
        ];
        assert!(!is_group_deduped(&g));

        let g = vec![FilePath {
            path: PathBuf::from("/foo/1.txt"),
            op: FileOp::Keep,
        }];
        assert!(is_group_deduped(&g));
    }
}
