use crate::error::AppError;
use crate::executor::Action;
use crate::scanner::scan;
use chrono::{DateTime, FixedOffset, Local};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;

pub mod textformat;
pub mod validation;

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug)]
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
}

pub struct Snapshot {
    pub rootdir: PathBuf,
    generated_at: DateTime<FixedOffset>,
    duplicates: HashMap<u64, Vec<FilePath>>,
}

impl Snapshot {
    pub fn of_rootdir(
        rootdir: &PathBuf,
        excludes: Option<&HashSet<PathBuf>>,
        quick: &bool,
    ) -> io::Result<Snapshot> {
        let duplicates = scan(rootdir, excludes, quick)?
            .into_iter()
            .map(|(d, ps)| (d, ps.into_iter().map(FilePath::new).collect()))
            .collect::<HashMap<u64, Vec<FilePath>>>();
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
