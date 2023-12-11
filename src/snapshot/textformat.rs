use super::Snapshot;

#[allow(dead_code)]
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


    fn decode(_s: String) -> Self {
        // To be implemented
        Line::Blank
    }
}

fn render_lines(snap: &Snapshot) -> Vec<Line> {
    // @TODO: Can we calculate the no. of lines roughly and initialize
    // a vector with that capacity?
    let mut lines: Vec<Line> = Vec::new();
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
