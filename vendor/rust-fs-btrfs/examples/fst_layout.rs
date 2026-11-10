//! How the free-space tree distributes its items across leaves.
//!
//! A previous attempt to update the tree assumed a `FREE_SPACE_INFO`
//! item and the `FREE_SPACE_EXTENT` items for its block group live
//! together in one leaf, so a leaf could be rewritten knowing only the
//! group. That was assumed rather than measured and it was wrong. This
//! prints what is actually there.
//!
//!   cargo run --example fst_layout -- image.img [...]

use fs_btrfs::btree::{header_offsets as o, HEADER_SIZE};
use fs_btrfs::fs::Filesystem;
use fs_core::FileDevice;
use std::sync::Arc;

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn name(t: u8) -> &'static str {
    match t {
        198 => "INFO",
        199 => "EXTENT",
        200 => "BITMAP",
        _ => "?",
    }
}

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(dev) = FileDevice::open(&path) else {
            continue;
        };
        let Ok(fs) = Filesystem::mount(Arc::new(dev)) else {
            continue;
        };
        let sb = fs.superblock().clone();
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let n = sb.nodesize as usize;

        println!("\n=== {path}  nodesize {n}");
        let groups = fs.block_groups().unwrap_or_default();
        println!("  {} block groups", groups.len());

        let mut at = 0usize;
        let mut leaves = 0usize;
        while at + n <= bytes.len() {
            let b = &bytes[at..at + n];
            at += n;
            if b[o::FSID..o::FSID + 16] != sb.fsid[..] || b[o::LEVEL] != 0 {
                continue;
            }
            if le64(b, o::OWNER) != 10 || !sb.csum_type.verify(&b[32..], &b[..32]) {
                continue;
            }
            // Only the current version of each leaf.
            if le64(b, o::GENERATION) < sb.generation.saturating_sub(2) {
                continue;
            }
            let nritems = le32(b, o::NRITEMS) as usize;
            if nritems == 0 {
                continue;
            }
            leaves += 1;

            let mut kinds = std::collections::BTreeMap::<&str, usize>::new();
            let mut infos = Vec::new();
            let mut first = None;
            let mut last = 0u64;
            for i in 0..nritems {
                let p = HEADER_SIZE + i * 25;
                let oid = le64(b, p);
                let ty = b[p + 8];
                *kinds.entry(name(ty)).or_default() += 1;
                if ty == 198 {
                    infos.push(oid);
                }
                if first.is_none() {
                    first = Some(oid);
                }
                last = oid;
            }
            println!(
                "  leaf {:<10} gen {:<4} {nritems:>4} items {:?}  keys {}..{}",
                le64(b, o::BYTENR),
                le64(b, o::GENERATION),
                kinds,
                first.unwrap_or(0),
                last
            );
            if !infos.is_empty() {
                println!("      INFO for groups {infos:?}");
            }
        }
        if leaves == 0 {
            println!("  no free-space tree leaves");
        }
    }
}
