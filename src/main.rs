use crate::snapshot::Snapshot;
use std::path::Path;

mod fileutil;
mod snapshot;

fn main() {
    let dir = Path::new("/Users/vineet/Dropbox");
    let snap = Snapshot::of_rootdir(&dir).unwrap();
    snap.render_text();
}
