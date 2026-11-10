//! What the LIVE extent and free-space trees hold.
//!
//! Scanning a disk turns up leaves nothing points at any more. This
//! walks from the current roots, which is the only way to ask what the
//! filesystem believes right now.
use fs_btrfs::chunk::{key_type, objectid, DiskKey};
use fs_btrfs::fs::Filesystem;
use fs_core::FileDevice;
use std::sync::Arc;

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(dev) = FileDevice::open(&path) else {
            continue;
        };
        let Ok(fs) = Filesystem::mount(Arc::new(dev)) else {
            continue;
        };
        println!("\n=== {path} generation {}", fs.superblock().generation);

        // The extent tree, through the crate's own walk.
        let mut groups = Vec::new();
        fs.for_each_extent_item(&mut |k: &DiskKey, _: &[u8]| {
            if k.key_type == key_type::BLOCK_GROUP_ITEM {
                groups.push((k.objectid, k.offset));
            }
        })
        .unwrap();
        println!("  extent tree: {} BLOCK_GROUP_ITEMs", groups.len());
        for (start, len) in &groups {
            println!("      start {start:<12} len {len}");
        }

        // The free-space tree, walked the same way.
        let Ok(root) = fs.tree_root_public(objectid::FREE_SPACE_TREE) else {
            println!("  no free-space tree");
            continue;
        };
        let mut infos = Vec::new();
        let mut extents = 0usize;
        fs.for_each_item_in(root, &mut |k: &DiskKey, _: &[u8]| match k.key_type {
            198 => infos.push((k.objectid, k.offset)),
            199 => extents += 1,
            _ => {}
        })
        .unwrap();
        println!("  free-space tree: {} INFO, {extents} EXTENT", infos.len());
        for (start, len) in &infos {
            let known = groups.iter().any(|(s, _)| s == start);
            println!(
                "      start {start:<12} len {len:<10} {}",
                if known { "" } else { "<- NO BLOCK GROUP" }
            );
        }
    }
}
