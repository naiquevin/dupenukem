use super::{find_keeper, FileOp, FilePath, Snapshot};
use crate::error::AppError;
use crate::fileutil::normalize_path;
use crate::hash::Checksum;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq)]
enum Line {
    Comment(String),
    MetaData {
        key: String,
        val: String,
    },
    Checksum(String),
    PathInfo {
        path: String,
        op: String,
        delim: Option<String>,
        extra: Option<String>,
    },
    Blank,
}

impl Line {
    fn encode(&self) -> String {
        match self {
            Self::Comment(comment) => format!("# {}", comment),
            Self::MetaData { key, val } => format!("#! {}: {}", key, val),
            Self::Checksum(hash) => format!("[{}]", hash),
            Self::PathInfo {
                path,
                op,
                delim,
                extra,
            } => {
                match &extra {
                    // @NOTE: Here we're not handling the case where
                    // delim is None. At this point it's not clear
                    // whether that would be a good idea.
                    Some(x) => format!("{} {} {} {}", op, path, delim.as_ref().unwrap(), x),
                    None => format!("{} {}", op, path),
                }
            }
            Self::Blank => String::from(""),
        }
    }

    fn decode(s: &str) -> Result<Self, AppError> {
        let cleaned = s.trim();
        let mut characters = cleaned.chars();
        match &characters.next() {
            Some('#') => {
                if characters.next().is_none() {
                    // Fist check if it's an empty, comment and handle
                    // it. Otherwise the `&cleaned[..2]` in the next
                    // condition could panic.
                    Ok(Self::Comment("".to_owned()))
                } else if &cleaned[..2] == "#!" {
                    let re = Regex::new(r"^#!\s*([^:]+):\s*(.+)$").unwrap();
                    let caps = re.captures(cleaned).ok_or(AppError::SnapshotParsing)?;
                    let key = caps
                        .get(1)
                        .ok_or(AppError::SnapshotParsing)?
                        .as_str()
                        .to_owned();
                    let val = caps
                        .get(2)
                        .ok_or(AppError::SnapshotParsing)?
                        .as_str()
                        .to_owned();
                    Ok(Self::MetaData { key, val })
                } else {
                    let re = Regex::new(r"^#\s(.+)$").unwrap();
                    let caps = re.captures(cleaned).ok_or(AppError::SnapshotParsing)?;
                    let comment = caps
                        .get(1)
                        .ok_or(AppError::SnapshotParsing)?
                        .as_str()
                        .to_owned();
                    Ok(Self::Comment(comment))
                }
            }
            Some('[') => {
                let re = Regex::new(r"^\[([^\]]+)\]").unwrap();
                let caps = re.captures(cleaned).ok_or(AppError::SnapshotParsing)?;
                let hash = caps
                    .get(1)
                    .ok_or(AppError::SnapshotParsing)?
                    .as_str()
                    .to_owned();
                Ok(Self::Checksum(hash))
            }
            Some(_) => {
                let re = Regex::new(r"^(keep|symlink|delete)\s(.+)$").unwrap();
                let caps = re.captures(cleaned).ok_or(AppError::SnapshotParsing)?;
                let op = caps
                    .get(1)
                    .ok_or(AppError::SnapshotParsing)?
                    .as_str()
                    .to_owned();
                let path = caps
                    .get(2)
                    .ok_or(AppError::SnapshotParsing)?
                    .as_str()
                    .to_owned();
                if op == "symlink" {
                    let parts: Vec<&str> = path
                        .split("->")
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if parts.len() == 2 {
                        let target = String::from(parts[0]);
                        let src = String::from(parts[1]);
                        Ok(Self::PathInfo {
                            op,
                            path: target,
                            delim: Some(String::from("->")),
                            extra: Some(src),
                        })
                    } else if parts.len() == 1 {
                        let target = String::from(parts[0]);
                        Ok(Self::PathInfo {
                            op,
                            path: target,
                            delim: Some(String::from("->")),
                            extra: None,
                        })
                    } else {
                        Err(AppError::SnapshotParsing)
                    }
                } else {
                    Ok(Self::PathInfo {
                        op,
                        path,
                        delim: None,
                        extra: None,
                    })
                }
            }
            None => Ok(Self::Blank),
        }
    }

    // Constructor of sorts to create PathInfo variant from a
    // `FilePath` instance
    fn pathinfo(filepath: &FilePath, rootdir: &Path) -> Self {
        // The `path` field in `Self::PathInfo` must be a relative
        // path, so we first compute that using the rootdir
        let path = normalize_path(&filepath.path, true, rootdir)
            // assuming that `rootdir` is an ancestor of the path
            .unwrap()
            .to_str()
            // assuming that path is a valid unicode
            .unwrap()
            .to_owned();
        let op = filepath.op.keyword().to_owned();
        match &filepath.op {
            FileOp::Symlink { source } => {
                let delim = Some(String::from("->"));
                let extra = source.as_ref().map(|s| s.display().to_string());
                Line::PathInfo {
                    path,
                    op,
                    delim,
                    extra,
                }
            }
            FileOp::Keep | FileOp::Delete => Line::PathInfo {
                path,
                op,
                delim: None,
                extra: None,
            },
        }
    }
}

/// Sort entries in the duplicate groups hashmap by size
///
/// Note that it returns a vector of tuples
fn sorted_groups(
    duplicates: &HashMap<Checksum, Vec<FilePath>>,
) -> Vec<(&Checksum, &Vec<FilePath>)> {
    let mut dups = duplicates
        .iter()
        .map(|x| {
            let size = find_keeper(x.1).and_then(|fp| fp.size().ok()).unwrap_or(0);
            (x.0, x.1, size)
        })
        .collect::<Vec<(&Checksum, &Vec<FilePath>, u64)>>();
    dups.sort_by(|a, b| b.2.cmp(&a.2));
    dups.iter()
        .map(|x| (x.0, x.1))
        .collect::<Vec<(&Checksum, &Vec<FilePath>)>>()
}

fn render_lines(snap: &Snapshot) -> Vec<Line> {
    // When there are no duplicates, there is nothing to return. The
    // caller code may check for an empty return value and log a
    // user friendly message
    if snap.duplicates.is_empty() {
        return vec![];
    }

    // @TODO: Can we calculate the no. of lines roughly and initialize
    // a vector with that capacity?
    let mut lines: Vec<Line> = Vec::new();

    // Add root dir as metadata
    lines.push(Line::MetaData {
        key: "Root Directory".to_string(),
        val: snap.rootdir.display().to_string(),
    });

    // Add time of generation as metadata
    lines.push(Line::MetaData {
        key: "Generated at".to_string(),
        val: snap.generated_at.to_rfc2822(),
    });

    // Add a blank line before dumping the filepath groupings
    lines.push(Line::Blank);

    for (ck, vs) in sorted_groups(&snap.duplicates) {
        lines.push(Line::Checksum(format!("{}", ck)));
        for v in vs {
            lines.push(Line::pathinfo(v, &snap.rootdir));
        }
        lines.push(Line::Blank);
    }

    let help = vec![
        "Reference:",
        "keep <target> = keep the target path as it is",
        "delete <target> = delete the target path",
        "symlink <target> [-> <src>] = Replace target with a symlink",
        ".       If 'src' is specified, it can either be an absolute or",
        ".       relative (to 'target'). Else one of the duplicates marked",
        ".       as 'keep' will be considered. If 'src' is not specified,",
        ".       a relative symlink will be created.",
        "",
        "This section is a comment and will be ignored by the tool",
    ];

    for help_line in help {
        lines.push(Line::Comment(help_line.to_string()));
    }

    lines
}

pub fn render(snap: &Snapshot) -> Vec<String> {
    let lines = render_lines(snap);
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines.iter() {
        result.push(line.encode());
    }
    result
}

pub fn parse(str_lines: Vec<String>) -> Result<Snapshot, AppError> {
    let lines = str_lines.iter().map(|s| Line::decode(s.as_str()));
    let mut rootdir: Option<PathBuf> = None;
    let mut generated_at: Option<DateTime<FixedOffset>> = None;
    let mut curr_group: Option<u64> = None;
    let mut duplicates: HashMap<Checksum, Vec<FilePath>> = HashMap::new();
    for line in lines {
        match &line {
            Ok(Line::Comment(_)) => continue,
            Ok(Line::Blank) => continue,
            Ok(Line::MetaData { key, val }) => {
                if key == "Root Directory" {
                    rootdir = Some(PathBuf::from(val));
                } else if key == "Generated at" {
                    generated_at = Some(DateTime::parse_from_rfc2822(val).unwrap());
                }
            }
            Ok(Line::Checksum(hash)) => {
                let parsed_checksum =
                    Checksum::parse(hash.as_str()).map_err(|_| AppError::SnapshotParsing)?;
                curr_group = Some(parsed_checksum.value())
            }
            Ok(Line::PathInfo {
                path,
                op,
                delim: _,
                extra,
            }) => {
                let group = Checksum::new(curr_group.unwrap());
                // `clone` is called below because `ok_or` causes a move
                let base_dir = rootdir.clone().ok_or(AppError::SnapshotParsing)?;
                let path = PathBuf::from(path);
                let abs_path = normalize_path(&path, false, &base_dir)?;
                let filepath = FilePath {
                    path: abs_path,
                    op: FileOp::decode(op.as_str(), extra.as_ref().map(|s| s.as_str())).unwrap(),
                };
                if let Some(fps) = duplicates.get_mut(&group) {
                    fps.push(filepath);
                } else {
                    duplicates.insert(group, vec![filepath]);
                }
            }
            Err(_) => return Err(AppError::SnapshotParsing),
        }
    }
    Ok(Snapshot {
        rootdir: rootdir.ok_or(AppError::SnapshotParsing)?,
        generated_at: generated_at.ok_or(AppError::SnapshotParsing)?,
        duplicates,
    })
}

#[cfg(test)]
mod tests {

    use super::*;

    // Tests for Line enum methods

    #[test]
    fn test_line_decode_blank() {
        let x = Line::decode(&"".to_owned());
        assert!(x.is_ok());
        assert_eq!(Line::Blank, x.unwrap());

        let x = Line::decode(&"  ".to_owned());
        assert!(x.is_ok());
        assert_eq!(Line::Blank, x.unwrap());
    }

    #[test]
    fn test_line_decode_comment() {
        let x = Line::decode(&"# This is a comment".to_owned());
        assert!(x.is_ok());
        assert_eq!(Line::Comment("This is a comment".to_owned()), x.unwrap());

        let x = Line::decode(&"# ".to_owned());
        assert!(x.is_ok());
        assert_eq!(Line::Comment("".to_owned()), x.unwrap());
    }

    #[test]
    fn test_line_decode_metadata() {
        let x = Line::decode(&"#! Root Directory: /path/to/rootdir".to_owned());
        assert!(x.is_ok());
        assert_eq!(
            Line::MetaData {
                key: "Root Directory".to_owned(),
                val: "/path/to/rootdir".to_owned(),
            },
            x.unwrap()
        );

        // Without space after colon
        let x = Line::decode(&"#! Root Directory:/path/to/rootdir".to_owned());
        assert!(x.is_ok());
        assert_eq!(
            Line::MetaData {
                key: "Root Directory".to_owned(),
                val: "/path/to/rootdir".to_owned(),
            },
            x.unwrap()
        );

        // Without space after exclamation
        let x = Line::decode(&"#!Root Directory:/path/to/rootdir".to_owned());
        assert!(x.is_ok());
        assert_eq!(
            Line::MetaData {
                key: "Root Directory".to_owned(),
                val: "/path/to/rootdir".to_owned(),
            },
            x.unwrap()
        );

        // Unrecognized metadata
        let x = Line::decode(&"#! Foo: bar".to_owned());
        assert!(x.is_ok());
        assert_eq!(
            Line::MetaData {
                key: "Foo".to_owned(),
                val: "bar".to_owned(),
            },
            x.unwrap()
        );

        // When `#!` prefix is incorrectly used
        match Line::decode(&"#!".to_owned()) {
            Err(AppError::SnapshotParsing) => assert!(true),
            Err(_) => assert!(false),
            Ok(_) => assert!(false),
        }

        match Line::decode(&"#! Just a comment by mistake".to_owned()) {
            Err(AppError::SnapshotParsing) => assert!(true),
            Err(_) => assert!(false),
            Ok(_) => assert!(false),
        }

        match Line::decode(&"#! Empty metadata: ".to_owned()) {
            Err(AppError::SnapshotParsing) => assert!(true),
            Err(_) => assert!(false),
            Ok(_) => assert!(false),
        }
    }

    #[test]
    fn test_line_decode_checksum() {
        let x = Line::decode(&"[fd2dd43f6cd0565ed876ca1ac2dfc708]".to_owned());
        match x {
            Ok(Line::Checksum(d)) => {
                assert_eq!("fd2dd43f6cd0565ed876ca1ac2dfc708".to_owned(), d);
            }
            _ => assert!(false),
        }
    }

    #[test]
    fn test_line_decode_pathinfo() {
        // keep
        let x = Line::decode(&"keep /foo/bar/1.txt".to_owned());
        assert!(x.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "keep".to_owned(),
                delim: None,
                extra: None,
            },
            x.unwrap()
        );

        // symlink
        let y = Line::decode(&"symlink /foo/bar/1.txt".to_owned());
        assert!(y.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "symlink".to_owned(),
                delim: Some("->".to_owned()),
                extra: None,
            },
            y.unwrap()
        );

        let y = Line::decode(&"symlink /foo/bar/1.txt -> /foo/cat/1.txt".to_owned());
        assert!(y.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "symlink".to_owned(),
                delim: Some("->".to_owned()),
                extra: Some("/foo/cat/1.txt".to_owned()),
            },
            y.unwrap()
        );

        let y = Line::decode(&"symlink /foo/bar/1.txt ->".to_owned());
        assert!(y.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "symlink".to_owned(),
                delim: Some("->".to_owned()),
                extra: None,
            },
            y.unwrap()
        );

        match Line::decode(&"symlink /foo/bar/1.txt -> /cat/1.txt -> /dog/2.txt".to_owned()) {
            Err(AppError::SnapshotParsing) => assert!(true),
            Err(_) => assert!(false),
            Ok(_) => assert!(false),
        }

        // delete
        let z = Line::decode(&"delete /foo/bar/1.txt".to_owned());
        assert!(z.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "delete".to_owned(),
                delim: None,
                extra: None,
            },
            z.unwrap()
        );

        // with unknown marker
        match Line::decode(&"create /foo/bar/1.txt".to_owned()) {
            Err(AppError::SnapshotParsing) => assert!(true),
            Err(_) => assert!(false),
            Ok(_) => assert!(false),
        }
    }

    #[test]
    fn test_line_pathinfo() {
        let rootdir = PathBuf::from("/base_dir");

        // Symlink with extra
        let t = PathBuf::from("/base_dir/bar/1.txt");
        let s = PathBuf::from("../foo/1.txt");
        let op = FileOp::Symlink { source: Some(s) };
        let fp = FilePath { path: t, op };
        let line = Line::pathinfo(&fp, &rootdir);
        assert_eq!(
            Line::PathInfo {
                path: "bar/1.txt".to_owned(),
                op: "symlink".to_owned(),
                delim: Some("->".to_owned()),
                extra: Some("../foo/1.txt".to_owned()),
            },
            line
        );

        // Symlink without extra
        let path = PathBuf::from("/base_dir/foo/1.txt");
        let op = FileOp::Symlink { source: None };
        let fp = FilePath { path, op };
        let line = Line::pathinfo(&fp, &rootdir);
        assert_eq!(
            Line::PathInfo {
                path: "foo/1.txt".to_owned(),
                op: "symlink".to_owned(),
                delim: Some("->".to_owned()),
                extra: None,
            },
            line
        );

        // Keep
        let path = PathBuf::from("/base_dir/foo/1.txt");
        let op = FileOp::Keep;
        let fp = FilePath { path, op };
        let line = Line::pathinfo(&fp, &rootdir);
        assert_eq!(
            Line::PathInfo {
                path: "foo/1.txt".to_owned(),
                op: "keep".to_owned(),
                delim: None,
                extra: None,
            },
            line
        );

        // Delete
        let path = PathBuf::from("/base_dir/foo/1.txt");
        let op = FileOp::Delete;
        let fp = FilePath { path, op };
        let line = Line::pathinfo(&fp, &rootdir);
        assert_eq!(
            Line::PathInfo {
                path: "foo/1.txt".to_owned(),
                op: "delete".to_owned(),
                delim: None,
                extra: None,
            },
            line
        );
    }

    // Tests for `parse` method

    #[test]
    fn test_parse() {
        let input = vec![
            "#! Root Directory: /foo",
            "#! Generated at: Tue, 12 Dec 2023 16:00:44 +0530",
            "",
            "[937219074347857651]",
            "symlink /foo/bar/1.txt",
            "keep /foo/1.txt",
            "delete /foo/bar/1_copy.txt",
            "",
            "[8183168229739997842]",
            "keep /foo/2.txt",
            "symlink /foo/bar/2.txt",
        ];
        let lines = input.iter().map(|s| String::from(*s)).collect();
        let snap: Snapshot = parse(lines).unwrap();
        assert_eq!(PathBuf::from("/foo"), snap.rootdir);

        let d1 = Checksum::parse("937219074347857651").unwrap();
        if let Some(fps) = snap.duplicates.get(&d1) {
            assert_eq!(3, fps.len());
            // 1st filepath
            assert_eq!(FileOp::Symlink { source: None }, fps[0].op);
            assert_eq!(
                "/foo/bar/1.txt".to_owned(),
                fps[0].path.display().to_string()
            );

            // 2nd filepath
            assert_eq!(FileOp::Keep, fps[1].op);
            assert_eq!("/foo/1.txt".to_owned(), fps[1].path.display().to_string());

            // 3rd filepath
            assert_eq!(FileOp::Delete, fps[2].op);
            assert_eq!(
                "/foo/bar/1_copy.txt".to_owned(),
                fps[2].path.display().to_string()
            );
        } else {
            assert!(false);
        }

        let d2 = Checksum::parse("8183168229739997842").unwrap();
        if let Some(fps) = snap.duplicates.get(&d2) {
            assert_eq!(2, fps.len());
        }
    }
}
