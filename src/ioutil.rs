use std::fs::File;
use std::io::{self, BufRead};
use std::path::PathBuf;

pub fn stdin_to_vec() -> io::Result<Vec<String>> {
    let stdin = io::stdin();
    let mut result = Vec::new();
    for line in stdin.lines() {
        let s = line?;
        result.push(s);
    }
    Ok(result)
}

pub fn read_lines_in_file(path: &PathBuf) -> io::Result<Vec<String>> {
    let file = File::open(path)?;
    let mut result = Vec::new();
    for line in io::BufReader::new(file).lines() {
        let s = line?;
        result.push(s);
    }
    Ok(result)
}
