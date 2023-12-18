use super::{FileOp, FilePath, Snapshot};
use crate::error::AppError;
use chrono::{DateTime, FixedOffset};
use hex::{self, FromHexError};
use md5::Digest;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Eq, PartialEq)]
enum Line {
    Comment(String),
    MetaData { key: String, val: String },
    Checksum(String),
    PathInfo { path: String, op: String },
    Blank,
}

impl Line {
    fn encode(&self) -> String {
        match self {
            Self::Comment(comment) => format!("# {}", comment),
            Self::MetaData { key, val } => format!("#! {}: {}", key, val),
            Self::Checksum(hash) => format!("[{}]", hash),
            Self::PathInfo { path, op } => format!("{} {}", op, path),
            Self::Blank => String::from(""),
        }
    }

    fn decode(s: &String) -> Result<Self, AppError> {
        let cleaned = s.trim();
        let mut characters = cleaned.chars();
        match &characters.next() {
            Some('#') => {
                if let None = &characters.next() {
                    // Fist check if it's an empty, comment and handle
                    // it. Otherwise the `&cleaned[..2]` in the next
                    // condition could panic.
                    Ok(Self::Comment("".to_owned()))
                } else if &cleaned[..2] == "#!" {
                    let re = Regex::new(r"^#!\s*([^:]+):\s*(.+)$").unwrap();
                    let caps = re.captures(&cleaned).ok_or(AppError::SnapshotParsing)?;
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
                    let caps = re.captures(&cleaned).ok_or(AppError::SnapshotParsing)?;
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
                let caps = re.captures(&cleaned).ok_or(AppError::SnapshotParsing)?;
                let hash = caps
                    .get(1)
                    .ok_or(AppError::SnapshotParsing)?
                    .as_str()
                    .to_owned();
                Ok(Self::Checksum(hash))
            }
            Some(_) => {
                let re = Regex::new(r"^(keep|symlink|delete)\s(.+)$").unwrap();
                let caps = re.captures(&cleaned).ok_or(AppError::SnapshotParsing)?;
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
                Ok(Self::PathInfo { op, path })
            }
            None => Ok(Self::Blank),
        }
    }
}

fn render_lines(snap: &Snapshot) -> Vec<Line> {
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

    for (k, vs) in snap.duplicates.iter() {
        lines.push(Line::Checksum(format!("{:x}", k)));
        for v in vs {
            lines.push(Line::PathInfo {
                path: v.path.to_str().unwrap().to_owned(),
                op: v.op.to_string(),
            });
        }
        lines.push(Line::Blank);
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

fn str_to_digest(s: &str) -> Result<Digest, FromHexError> {
    let mut bytea = [0u8; 16];
    hex::decode_to_slice(s, &mut bytea)?;
    Ok(Digest(bytea))
}

#[allow(dead_code)]
pub fn parse(str_lines: Vec<String>) -> Result<Snapshot, AppError> {
    let lines = str_lines.iter().map(Line::decode);
    let mut rootdir: Option<PathBuf> = None;
    let mut generated_at: Option<DateTime<FixedOffset>> = None;
    let mut curr_group: Option<Digest> = None;
    let mut duplicates: HashMap<Digest, Vec<FilePath>> = HashMap::new();
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
                curr_group = str_to_digest(hash.as_str()).ok();
            }
            Ok(Line::PathInfo { path, op }) => {
                let group = curr_group.unwrap();
                let filepath = FilePath {
                    path: PathBuf::from(path),
                    op: FileOp::decode(op.as_str()).unwrap(),
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
        let x = Line::decode(&"keep /foo/bar/1.txt".to_owned());
        assert!(x.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "keep".to_owned(),
            },
            x.unwrap()
        );

        let y = Line::decode(&"symlink /foo/bar/1.txt".to_owned());
        assert!(y.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "symlink".to_owned(),
            },
            y.unwrap()
        );

        let z = Line::decode(&"delete /foo/bar/1.txt".to_owned());
        assert!(z.is_ok());
        assert_eq!(
            Line::PathInfo {
                path: "/foo/bar/1.txt".to_owned(),
                op: "delete".to_owned(),
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

    // Tests for `parse` method

    #[test]
    fn test_parse() {
        let input = vec![
            "#! Root Directory: /foo",
            "#! Generated at: Tue, 12 Dec 2023 16:00:44 +0530",
            "",
            "[fd2dd43f6cd0565ed876ca1ac2dfc708]",
            "symlink /foo/bar/1.txt",
            "keep /foo/1.txt",
            "delete /foo/bar/1_copy.txt",
            "",
            "[b2c7374428473edcfd949a6fd3bbe7d1]",
            "keep /foo/2.txt",
            "symlink /foo/bar/2.txt",
        ];
        let lines = input.iter().map(|s| String::from(*s)).collect();
        let snap: Snapshot = parse(lines).unwrap();
        assert_eq!(PathBuf::from("/foo"), snap.rootdir);

        let d1 = str_to_digest("fd2dd43f6cd0565ed876ca1ac2dfc708").unwrap();
        if let Some(fps) = snap.duplicates.get(&d1) {
            assert_eq!(3, fps.len());
            // 1st filepath
            assert_eq!(FileOp::Symlink, fps[0].op);
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

        let d2 = str_to_digest("b2c7374428473edcfd949a6fd3bbe7d1").unwrap();
        if let Some(fps) = snap.duplicates.get(&d2) {
            assert_eq!(2, fps.len());
        }
    }
}
