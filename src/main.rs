use crate::fileutil::{traverse_bfs, find_duplicates};
use md5::{self, Digest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod fileutil;

fn render_output(dups: &HashMap<Digest, Vec<&PathBuf>>) {
    for (k, vs) in dups.iter() {
        println!("[{:x}]", k);
        for v in vs {
            println!("F {}", v.display());
        }
        println!("");
    }
}

fn main() {
    let dir = Path::new("/Users/vineet/Dropbox");
    let paths = traverse_bfs(&dir).unwrap();
    let dups = find_duplicates(&paths).unwrap();
    render_output(&dups);
}
