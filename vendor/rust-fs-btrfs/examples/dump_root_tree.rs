//! Dump the root tree's items, so what is in it can be read rather than
//! assumed.
//!
//! `cargo run --example dump_root_tree -- <image>`

use fs_btrfs::fs::Filesystem;
use fs_core::FileDevice;
use std::sync::Arc;

fn main() {
    let img = std::env::args()
        .nth(1)
        .expect("usage: dump_root_tree <image>");
    let fs = Filesystem::mount(Arc::new(FileDevice::open(&img).expect("open"))).expect("mount");

    for (objectid, key_type, offset, data) in fs.root_tree_items().expect("walk") {
        #[allow(clippy::needless_continue)]
        let tail: String = data
            .iter()
            .rev()
            .take(20)
            .rev()
            .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
            .collect();
        // Decode the fields a subvolume listing needs, for the trees a
        // listing would show. The offsets are a hypothesis being tested:
        // an embedded 160-byte inode item, then generation, root_dirid,
        // bytenr — which puts flags at 208.
        if key_type == 132 && (objectid == 5 || objectid >= 256) && data.len() > 216 {
            let le64 = |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
            println!(
                "  ROOT_ITEM id {objectid:>4}  gen {:>3}  bytenr {:>9}  flags {:#06x}  last_snapshot {}",
                le64(160),
                le64(176),
                le64(208),
                le64(200)
            );
            continue;
        }
        println!(
            "  objectid {objectid:>4}  type {key_type:>3}  offset {offset:>5}  len {:>4}  tail {tail:?}",
            data.len()
        );
    }
}
