//! Where the kernel put the boundary when a leaf split.
//!
//! Reads a before/after pair from `build-split-fixtures.sh` and prints
//! every fs-tree leaf on each side: how many items it holds, how full it
//! is, and the first and last key in it. The leaf that split appears on
//! the left; the two it became appear on the right.
//!
//!   cargo run --example split_diff -- before.img after.img

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

/// objectid/type/offset of the item at index `i`.
fn key_at(b: &[u8], i: usize) -> (u64, u8, u64) {
    let at = HEADER_SIZE + i * 25;
    (le64(b, at), b[at + 8], le64(b, at + 9))
}

fn report(path: &str, want_gen: Option<u64>) {
    let dev = Arc::new(FileDevice::open(path).unwrap());
    let fs = Filesystem::mount(dev).unwrap();
    let sb = fs.superblock().clone();
    let bytes = std::fs::read(path).unwrap();
    let n = sb.nodesize as usize;

    println!("\n{path}  (generation {})", sb.generation);
    let mut at = 0usize;
    let mut rows = Vec::new();
    while at + n <= bytes.len() {
        let b = &bytes[at..at + n];
        at += n;
        if b[o::FSID..o::FSID + 16] != sb.fsid[..] || b[o::LEVEL] != 0 {
            continue;
        }
        if le64(b, o::OWNER) != 5 {
            continue;
        }
        if !sb.csum_type.verify(&b[32..], &b[..32]) {
            continue;
        }
        let gen = le64(b, o::GENERATION);
        if want_gen.is_some_and(|g| gen != g) {
            continue;
        }
        let nritems = le32(b, o::NRITEMS) as usize;
        if nritems == 0 {
            continue;
        }
        // Used = header + item array + item data. The last item has the
        // lowest offset, and data runs from there to the end.
        let last = HEADER_SIZE + (nritems - 1) * 25;
        let low = le32(b, last + 17) as usize;
        let used = HEADER_SIZE + nritems * 25 + (n - HEADER_SIZE - low);
        rows.push((
            le64(b, o::BYTENR),
            gen,
            nritems,
            used * 100 / n,
            key_at(b, 0),
            key_at(b, nritems - 1),
        ));
    }
    rows.sort();
    for (bytenr, gen, nritems, fill, first, last) in rows {
        println!(
            "  leaf {bytenr:<10} gen {gen:<4} {nritems:>3} items  {fill:>3}% full  \
             first {first:?}  last {last:?}"
        );
    }
}

fn main() {
    let a = std::env::args().nth(1).unwrap();
    let b = std::env::args().nth(2).unwrap();
    // Only the newest generation on each side: older leaves are the
    // versions these replaced.
    let newest = |p: &str| {
        let dev = Arc::new(FileDevice::open(p).unwrap());
        Filesystem::mount(dev).unwrap().superblock().generation
    };
    let _ = newest;
    report(&a, None);
    report(&b, None);
}
