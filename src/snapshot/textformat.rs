use super::Snapshot;
use regex::Regex;

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
        val: snap.generated_at.to_string(),
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

#[cfg(test)]
mod tests {

    use super::*;

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
}
