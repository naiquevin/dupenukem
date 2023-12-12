use super::{Snapshot, FilePath, FileOp};
use chrono::{DateTime, FixedOffset};
use md5::Digest;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
enum Line {
    Comment(String),
    MetaData { key: String, val: String },
    Checksum(String),
    PathInfo { path: String, op: String },
    Blank,
}

#[allow(dead_code)]
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

    fn decode(s: &String) -> Self {
        let cleaned = s.trim();
        match &cleaned.chars().next() {
            Some('#') => {
                // Either a comment or MetaData
                if &cleaned[..2] == "#!" {
                    let re = Regex::new(r"^#!\s*([^:]+):\s*(.+)$").unwrap();
                    let caps = re.captures(&cleaned).unwrap();
                    Self::MetaData {
                        key: caps.get(1).unwrap().as_str().to_owned(),
                        val: caps.get(2).unwrap().as_str().to_owned(),
                    }
                } else {
                    let re = Regex::new(r"^#\s(.+)$").unwrap();
                    let caps = re.captures(&cleaned).unwrap();
                    Self::Comment(caps.get(1).unwrap().as_str().to_owned())
                }
            },
            Some('[') => {
                let re = Regex::new(r"^\[([^\]]+)\]").unwrap();
                let caps = re.captures(&cleaned).unwrap();
                Self::Checksum(caps.get(1).unwrap().as_str().to_owned())
            },
            Some(_) => {
                let re = Regex::new(r"^(keep|symlink|delete)\s(.+)$").unwrap();
                let caps = re.captures(&cleaned).unwrap();
                Self::PathInfo {
                    op: caps.get(1).unwrap().as_str().to_owned(),
                    path: caps.get(2).unwrap().as_str().to_owned()
                }
            }
            None => Self::Blank
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
        val: snap.rootdir.display().to_string()
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
                op: v.op.to_string()
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

#[allow(dead_code)]
pub fn parse(str_lines: Vec<String>) -> Snapshot {
    let lines = str_lines.iter().map(Line::decode);
    let mut rootdir: Option<PathBuf> = None;
    let mut generated_at: Option<DateTime<FixedOffset>> = None;
    let mut curr_group: Option<Digest> = None;
    let mut duplicates: HashMap<Digest, Vec<FilePath>> = HashMap::new();
    for line in lines {
        match &line {
            Line::Comment(_) => continue,
            Line::Blank => continue,
            Line::MetaData { key, val } => {
                if key == "Root Directory" {
                    rootdir = Some(PathBuf::from(val));
                } else if key == "Generated at" {
                    generated_at = Some(DateTime::parse_from_rfc2822(val).unwrap());
                }
            },
            Line::Checksum(hash) => {
                let mut bytea = [0u8; 16];
                hex::decode_to_slice(hash.as_str(), &mut bytea).unwrap();
                curr_group = Some(Digest(bytea));
            },
            Line::PathInfo { path, op } => {
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
            },
        }
    }
    Snapshot {
        rootdir: rootdir.unwrap(),
        generated_at: generated_at.unwrap(),
        duplicates,
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    // Tests for Line enum methods

    #[test]
    fn test_line_decode_blank() {
        let x = Line::decode(&"".to_owned());
        assert_eq!(Line::Blank, x);

        let x = Line::decode(&"  ".to_owned());
        assert_eq!(Line::Blank, x);
    }

    #[test]
    fn test_line_decode_comment() {
        let x = Line::decode(&"# This is a comment".to_owned());
        assert_eq!(Line::Comment("This is a comment".to_owned()), x);
    }

    #[test]
    fn test_line_decode_metadata() {
        let x = Line::decode(&"#! Root Directory: /path/to/rootdir".to_owned());
        assert_eq!(Line::MetaData {
            key: "Root Directory".to_owned(),
            val: "/path/to/rootdir".to_owned(),
        }, x);

        // Without space after colon
        let x = Line::decode(&"#! Root Directory:/path/to/rootdir".to_owned());
        assert_eq!(Line::MetaData {
            key: "Root Directory".to_owned(),
            val: "/path/to/rootdir".to_owned(),
        }, x);

        // Without space after exclamation
        let x = Line::decode(&"#!Root Directory:/path/to/rootdir".to_owned());
        assert_eq!(Line::MetaData {
            key: "Root Directory".to_owned(),
            val: "/path/to/rootdir".to_owned(),
        }, x);
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
        let snap: Snapshot = parse(lines);
        assert_eq!(PathBuf::from("/foo"), snap.rootdir);
    }
}
