//! Whether a transaction changes the dev tree's CONTENTS.
//!
//! The dev tree (objectid 4) holds DEV_EXTENT items mapping physical
//! device ranges to the chunks on them — the reverse of the chunk
//! tree's logical-to-physical map. A commit that changes nothing still
//! rewrites it, but rewriting a block is not the same as changing what
//! it says. This asks which.
//!
//!   cargo run --example dev_tree_diff -- before.img after.img
use fs_btrfs::chunk::{objectid, DiskKey};
use fs_btrfs::fs::Filesystem;
use fs_core::FileDevice;
use std::collections::BTreeMap;
use std::sync::Arc;

fn items(path: &str) -> BTreeMap<(u64, u8, u64), Vec<u8>> {
    let fs = Filesystem::mount(Arc::new(FileDevice::open(path).unwrap())).unwrap();
    let root = fs.tree_root_public(objectid::DEV_TREE).unwrap();
    let mut out = BTreeMap::new();
    fs.for_each_item_in(root, &mut |k: &DiskKey, d: &[u8]| {
        out.insert((k.objectid, k.key_type, k.offset), d.to_vec());
    })
    .unwrap();
    out
}

fn main() {
    let a = std::env::args().nth(1).unwrap();
    let b = std::env::args().nth(2).unwrap();
    let (x, y) = (items(&a), items(&b));
    println!("{}: {} dev tree items", a, x.len());
    println!("{}: {} dev tree items", b, y.len());

    let added: Vec<_> = y.keys().filter(|k| !x.contains_key(*k)).collect();
    let removed: Vec<_> = x.keys().filter(|k| !y.contains_key(*k)).collect();
    let changed: Vec<_> = x
        .keys()
        .filter(|k| y.contains_key(*k) && x[*k] != y[*k])
        .collect();

    println!("  added   {added:?}");
    println!("  removed {removed:?}");
    println!("  changed {changed:?}");
    if added.is_empty() && removed.is_empty() && changed.is_empty() {
        println!("  => IDENTICAL CONTENTS: the block moved, what it says did not");
    }
}
