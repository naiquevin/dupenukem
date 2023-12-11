use crate::fileutil::{traverse_bfs, find_duplicates};
use chrono::{DateTime, Local};
use md5::Digest;
use std::fmt;
use std::path::{PathBuf, Path};
use std::collections::HashMap;

mod textformat;

#[allow(dead_code)]
enum FileOp {
    Keep,
    Symlink,
    Delete,
}

impl fmt::Display for FileOp {

    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let c = match self {
            Self::Keep => "keep",
            Self::Symlink => "symlink",
            Self::Delete => "delete",
        };
        write!(f, "{}",c)
    }
}

struct FilePath {
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
            op
        }
    }
}

#[allow(dead_code)]
pub struct Snapshot {
    rootdir: PathBuf,
    generated_at: DateTime<Local>,
    duplicates: HashMap<Digest, Vec<FilePath>>
}

impl Snapshot {

    pub fn of_rootdir(rootdir: &Path) -> Snapshot {
        let paths = traverse_bfs(&rootdir).unwrap();
        let mut duplicates: HashMap<Digest, Vec<FilePath>> = HashMap::new();
        for (digest, paths) in find_duplicates(&paths).unwrap().iter() {
            let filepaths = paths.iter().map(|p| { FilePath::new(*p) }).collect();
            duplicates.insert(*digest, filepaths);
        }
        Snapshot {
            rootdir: rootdir.to_path_buf(),
            generated_at: Local::now(),
            duplicates
        }
    }

    pub fn render_text(&self) {
        for line in textformat::render(&self).iter() {
            println!("{}", line);
        }
    }
}
