use crate::error::AppError;
use crate::fileutil::{find_duplicates, traverse_bfs};
use chrono::{DateTime, FixedOffset, Local};
use md5::Digest;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;

pub mod execution;
pub mod textformat;
pub mod validation;

#[derive(Debug, PartialEq, Eq)]
enum FileOp {
    Keep,
    Symlink { source: Option<PathBuf> },
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

#[derive(Debug)]
pub struct FilePath {
    path: PathBuf,
    op: FileOp,
}

impl FilePath {
    fn new(path: &PathBuf) -> FilePath {
        let op = if path.is_symlink() {
            // @NOTE: Here we're not handling the case where
            // `canonicalize` returns an Err
            FileOp::Symlink {
                source: path.canonicalize().ok(),
            }
        } else {
            FileOp::Keep
        };
        FilePath {
            // @NOTE: This is equivalent to cloning
            path: path.to_path_buf(),
            op,
        }
    }
}

pub struct Snapshot {
    rootdir: PathBuf,
    generated_at: DateTime<FixedOffset>,
    duplicates: HashMap<Digest, Vec<FilePath>>,
}

impl Snapshot {
    pub fn of_rootdir(
        rootdir: &PathBuf,
        excludes: Option<&HashSet<PathBuf>>,
        quick: &bool,
    ) -> io::Result<Snapshot> {
        let paths = traverse_bfs(rootdir, excludes)?;
        let duplicates = find_duplicates(rootdir, &paths, quick)?
            .into_iter()
            .map(|(d, ps)| (d, ps.into_iter().map(FilePath::new).collect()))
            .collect::<HashMap<Digest, Vec<FilePath>>>();
        let snap = Snapshot {
            rootdir: rootdir.to_path_buf(),
            generated_at: Local::now().fixed_offset(),
            duplicates,
        };
        Ok(snap)
    }

    pub fn validate(&self) -> Result<Vec<Action>, AppError> {
        validation::validate(&self).map_err(AppError::SnapshotValidation)
    }
}

#[derive(Debug)]
pub enum Action<'a> {
    Keep(&'a PathBuf),
    Symlink {
        path: &'a PathBuf,
        source: &'a PathBuf,
        is_no_op: bool,
    },
    Delete {
        path: &'a PathBuf,
        is_no_op: bool,
    },
}

impl<'a> Action<'a> {
    pub fn log(&self, dry_run: &bool) -> Option<String> {
        match self {
            Self::Keep(_) => return None,
            Self::Symlink {
                path,
                source,
                is_no_op,
            } => {
                let mut res = String::from("");
                if *dry_run {
                    res.push_str("[DRY RUN] ");
                }
                if *is_no_op {
                    res.push_str("[NO-OP] ");
                }
                res.push_str(
                    format!(
                        "Replacing file with symlink: {} -> {}",
                        path.display(),
                        // Here we're assuming that the source will never be
                        // None
                        source.display(),
                    )
                    .as_str(),
                );
                Some(res)
            }
            Self::Delete { path, is_no_op } => {
                let mut res = String::from("");
                if *dry_run {
                    res.push_str("[DRY RUN] ");
                }
                if *is_no_op {
                    res.push_str("[NO-OP] ");
                }
                res.push_str(format!("Deleting file: {}", path.display()).as_str());
                Some(res)
            }
        }
    }
}
