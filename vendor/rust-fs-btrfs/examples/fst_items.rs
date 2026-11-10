//! Every item of the live free-space tree, in order.
use fs_btrfs::chunk::{objectid, DiskKey};
use fs_btrfs::fs::Filesystem;
use fs_core::FileDevice;
use std::sync::Arc;
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let fs = Filesystem::mount(Arc::new(FileDevice::open(&path).unwrap())).unwrap();
    let root = fs.tree_root_public(objectid::FREE_SPACE_TREE).unwrap();
    let groups: Vec<_> = fs.block_groups().unwrap();
    fs.for_each_item_in(root, &mut |k: &DiskKey, d: &[u8]| {
        let kind = match k.key_type {
            198 => "INFO",
            199 => "EXTENT",
            200 => "BITMAP",
            _ => "?",
        };
        let owner = groups
            .iter()
            .find(|g| k.objectid >= g.start && k.objectid < g.end())
            .map(|g| g.start.to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "  {kind:<7} objectid {:<10} offset {:<10} data {:<3} group {owner}",
            k.objectid,
            k.offset,
            d.len()
        );
    })
    .unwrap();
}
