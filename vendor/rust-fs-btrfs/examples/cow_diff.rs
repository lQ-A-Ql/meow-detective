//! Which blocks and items one transaction changed.
//!
//! Not part of the library. It answers "what did this commit actually
//! do", which is how `docs/cow-transaction.md` was written and how the
//! next question about a transaction should be answered too — the
//! alternative is reasoning about the kernel's behaviour from its
//! documentation and implementing something plausible.
//!
//!   cargo run --example cow_diff -- before.img after.img
//!
//! Build the images with `chore fixtures cow` (the real kernel makes
//! them, inside the fs-linux-test-harness VM).

use fs_btrfs::btree::header_offsets as o;
use fs_btrfs::chunk::{key_type, DiskKey};
use fs_btrfs::fs::Filesystem;
use fs_core::FileDevice;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn tree_name(owner: u64) -> &'static str {
    match owner {
        1 => "root",
        2 => "extent",
        3 => "chunk",
        4 => "dev",
        5 => "fs",
        6 => "csum",
        7 => "quota",
        8 => "uuid",
        9 => "free-space",
        10 => "free-space-tree",
        _ => "other",
    }
}

/// A block's identity: where it is and which transaction wrote it.
/// Two versions of the same address are different blocks.
type BlockId = (u64, u64);
/// What a block is: which tree owns it, and its height.
type BlockKind = (u64, u8);
/// An item's key, flattened for comparison.
type ItemKey = (u64, u8, u64);

/// Every valid tree block in an image, keyed by (bytenr, generation).
fn blocks(path: &str) -> (Filesystem, BTreeMap<BlockId, BlockKind>) {
    let dev = Arc::new(FileDevice::open(path).unwrap());
    let fs = Filesystem::mount(dev).unwrap();
    let sb = fs.superblock().clone();
    let bytes = std::fs::read(path).unwrap();
    let n = sb.nodesize as usize;
    let mut out = BTreeMap::new();
    let mut at = 0usize;
    while at + n <= bytes.len() {
        let b = &bytes[at..at + n];
        at += n;
        if b[o::FSID..o::FSID + 16] != sb.fsid[..] {
            continue;
        }
        if !sb.csum_type.verify(&b[32..], &b[..32]) {
            continue;
        }
        out.insert(
            (le64(b, o::BYTENR), le64(b, o::GENERATION)),
            (le64(b, o::OWNER), b[o::LEVEL]),
        );
    }
    (fs, out)
}

fn extent_items(fs: &Filesystem) -> BTreeMap<ItemKey, Vec<u8>> {
    let mut out = BTreeMap::new();
    fs.for_each_extent_item(&mut |k: &DiskKey, d: &[u8]| {
        out.insert((k.objectid, k.key_type, k.offset), d.to_vec());
    })
    .unwrap();
    out
}

fn main() {
    let a = std::env::args().nth(1).unwrap();
    let b = std::env::args().nth(2).unwrap();
    let (fa, ba) = blocks(&a);
    let (fb, bb) = blocks(&b);

    println!(
        "generation {} -> {}",
        fa.superblock().generation,
        fb.superblock().generation
    );
    println!(
        "root       {} -> {}",
        fa.superblock().root,
        fb.superblock().root
    );
    println!(
        "bytes_used {} -> {}",
        fa.superblock().bytes_used,
        fb.superblock().bytes_used
    );

    // Blocks live in the AFTER image that were not live before, by
    // (address, generation) — a rewritten block reuses neither.
    let keys_a: BTreeSet<_> = ba.keys().copied().collect();
    let keys_b: BTreeSet<_> = bb.keys().copied().collect();

    println!("\n--- tree blocks the commit WROTE (present after, not before) ---");
    let mut by_tree: BTreeMap<&str, Vec<(u64, u64, u8)>> = BTreeMap::new();
    for k in keys_b.difference(&keys_a) {
        let (owner, level) = bb[k];
        by_tree
            .entry(tree_name(owner))
            .or_default()
            .push((k.0, k.1, level));
    }
    for (t, mut v) in by_tree {
        v.sort();
        println!("  {t:<16} {} block(s)", v.len());
        for (bytenr, gen, level) in v {
            println!("      bytenr {bytenr:<12} gen {gen:<4} level {level}");
        }
    }

    // The extent tree is the interesting one: what did recording those
    // allocations look like?
    let ea = extent_items(&fa);
    let eb = extent_items(&fb);
    let ka: BTreeSet<_> = ea.keys().copied().collect();
    let kb: BTreeSet<_> = eb.keys().copied().collect();

    let name = |t: u8| match t {
        key_type::EXTENT_ITEM => "EXTENT_ITEM",
        key_type::METADATA_ITEM => "METADATA_ITEM",
        key_type::BLOCK_GROUP_ITEM => "BLOCK_GROUP_ITEM",
        176 => "TREE_BLOCK_REF",
        178 => "EXTENT_DATA_REF",
        182 => "SHARED_BLOCK_REF",
        184 => "SHARED_DATA_REF",
        _ => "?",
    };

    println!("\n--- extent tree items ADDED ---");
    for k in kb.difference(&ka) {
        println!("  {:<12} {:<18} offset {}", k.0, name(k.1), k.2);
    }
    println!("--- extent tree items REMOVED ---");
    for k in ka.difference(&kb) {
        println!("  {:<12} {:<18} offset {}", k.0, name(k.1), k.2);
    }
    println!("--- extent tree items CHANGED ---");
    for k in ka.intersection(&kb) {
        if ea[k] != eb[k] {
            println!("  {:<12} {:<18} offset {}", k.0, name(k.1), k.2);
            if k.1 == key_type::BLOCK_GROUP_ITEM {
                println!("      used {} -> {}", le64(&ea[k], 0), le64(&eb[k], 0));
            }
        }
    }
}
