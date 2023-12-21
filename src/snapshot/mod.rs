use crate::error::AppError;
use crate::fileutil::{find_duplicates, traverse_bfs};
use chrono::{DateTime, FixedOffset, Local};
use md5::Digest;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io;
use std::path::PathBuf;

pub mod textformat;
pub mod validation;

#[derive(Debug, PartialEq, Eq)]
enum FileOp {
    Keep,
    Symlink,
    Delete,
}

impl FileOp {
    fn decode(s: &str) -> Option<Self> {
        match s {
            "keep" => Some(Self::Keep),
            "symlink" => Some(Self::Symlink),
            "delete" => Some(Self::Delete),
            // @TODO: Throw an error here
            _ => None,
        }
    }
}

impl fmt::Display for FileOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let c = match self {
            Self::Keep => "keep",
            Self::Symlink => "symlink",
            Self::Delete => "delete",
        };
        write!(f, "{}", c)
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
            FileOp::Symlink
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
    ) -> io::Result<Snapshot> {
        let paths = traverse_bfs(rootdir, excludes)?;
        let mut duplicates: HashMap<Digest, Vec<FilePath>> = HashMap::new();
        for (digest, paths) in find_duplicates(rootdir, &paths)?.iter() {
            let filepaths = paths.iter().map(|p| FilePath::new(*p)).collect();
            duplicates.insert(*digest, filepaths);
        }
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
#[allow(dead_code)]
pub struct Action<'a> {
    filepath: &'a FilePath,
    is_no_op: bool,
}
