use crate::snapshot::{textformat, Snapshot};
use std::path::Path;
use log::info;

mod error;
mod fileutil;
mod snapshot;

fn main() {
    env_logger::init();
    let dir = Path::new("/Users/vineet/Dropbox");
    info!("Generating snapshot for dir: {}", dir.display());
    let snap = Snapshot::of_rootdir(&dir).unwrap();
    for line in textformat::render(&snap).iter() {
        println!("{}", line);
    }
}
