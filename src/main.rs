use crate::snapshot::{textformat, Snapshot};
use std::path::Path;

mod error;
mod fileutil;
mod snapshot;

fn main() {
    let dir = Path::new("/Users/vineet/Dropbox");
    let snap = Snapshot::of_rootdir(&dir).unwrap();
    for line in textformat::render(&snap).iter() {
        println!("{}", line);
    }
}
