//! The order a commit reaches the device in.
//!
//! This is the crash-consistency, not a performance detail. A commit
//! whose superblock reaches the device before the tree blocks it names
//! is not a slower commit; it is a filesystem that a power cut turns
//! into one pointing at blocks that were never written.
//!
//! The order is not reasoned out here. One live kernel commit was
//! recorded with `blktrace`, and `docs/transaction-format.md` writes
//! down what that trace showed:
//!
//!   tree blocks (every mirror) → flush → superblocks → flush
//!
//! So the device here records every operation in sequence and the
//! assertions are about that sequence. A real device cannot be asked
//! what order it saw things in, which is the whole reason this is a
//! recording one.
//!
//! The image the recorder reads through is a real one, built by `chore
//! fixtures` in the fs-linux-test-harness VM. A fixture that is not
//! there fails the test that wanted it: an ordering check that never
//! ran is indistinguishable from one that held.

use fs_btrfs::commit::PlacedBlock;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::Commit;
use fs_btrfs_test_support::fixture;
use fs_core::{BlockDevice, BlockRead, Error as CoreError, Result as CoreResult};
use std::sync::{Arc, Mutex};

/// What the device was asked to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Write { at: u64, len: usize },
    Flush,
}

/// A device that remembers the sequence, and passes reads through to a
/// real image so the filesystem under test is a real one.
struct Recorder {
    image: Vec<u8>,
    ops: Mutex<Vec<Op>>,
    written: Mutex<Vec<(u64, Vec<u8>)>>,
}

impl Recorder {
    fn new(image: Vec<u8>) -> Self {
        Recorder {
            image,
            ops: Mutex::new(Vec::new()),
            written: Mutex::new(Vec::new()),
        }
    }
    fn ops(&self) -> Vec<Op> {
        self.ops.lock().unwrap().clone()
    }
    fn writes_at(&self, at: u64) -> Vec<Vec<u8>> {
        self.written
            .lock()
            .unwrap()
            .iter()
            .filter(|(o, _)| *o == at)
            .map(|(_, b)| b.clone())
            .collect()
    }
}

impl BlockRead for Recorder {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> CoreResult<()> {
        let at = offset as usize;
        if at + buf.len() > self.image.len() {
            return Err(CoreError::OutOfBounds {
                offset,
                len: buf.len() as u64,
                size: self.image.len() as u64,
            });
        }
        buf.copy_from_slice(&self.image[at..at + buf.len()]);
        Ok(())
    }
    fn size_bytes(&self) -> u64 {
        self.image.len() as u64
    }
}

impl BlockDevice for Recorder {
    fn write_at(&self, offset: u64, buf: &[u8]) -> CoreResult<()> {
        self.ops.lock().unwrap().push(Op::Write {
            at: offset,
            len: buf.len(),
        });
        self.written.lock().unwrap().push((offset, buf.to_vec()));
        Ok(())
    }
    fn flush(&self) -> CoreResult<()> {
        self.ops.lock().unwrap().push(Op::Flush);
        Ok(())
    }
    fn is_writable(&self) -> bool {
        true
    }
}

/// The named fixture, mounted read-write through a recording device.
fn mounted(name: &str) -> (Filesystem, Arc<Recorder>) {
    let path = fixture(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let dev = Arc::new(Recorder::new(bytes));
    let fs = Filesystem::mount_rw(dev.clone() as Arc<dyn BlockDevice>)
        .unwrap_or_else(|e| panic!("mounting {} read-write: {e}", path.display()));
    (fs, dev)
}

/// Where the flushes fall, which is the whole claim.
#[test]
fn tree_blocks_land_before_the_first_flush_and_superblocks_between_the_two() {
    let (fs, dev) = mounted("btrfs-default.img");
    let nodesize = fs.superblock().nodesize as usize;

    // Two blocks, placed wherever the allocator says is free. What is
    // being tested is the ordering, so the contents need only be the
    // right size.
    let first = fs.find_metadata_block().expect("somewhere to put a block");
    let blocks = vec![
        PlacedBlock {
            logical: first,
            bytes: vec![0xAA; nodesize],
        },
        PlacedBlock {
            logical: first + nodesize as u64,
            bytes: vec![0xBB; nodesize],
        },
    ];

    fs.commit(
        &blocks,
        &Commit {
            generation: fs.superblock().generation + 1,
            // The tree root the volume already has: these blocks are
            // placeholders, and a commit reads its backup roots out of the
            // root tree it names, which has to parse (#78).
            root: fs.superblock().root,
            ..Default::default()
        },
    )
    .expect("committing");

    let ops = dev.ops();
    let flushes: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| **o == Op::Flush)
        .map(|(i, _)| i)
        .collect();

    assert_eq!(
        flushes.len(),
        2,
        "a commit has two barriers — one after the tree blocks and one \
         after the superblocks, which is the commit point. Got {ops:?}"
    );

    let (first_flush, second_flush) = (flushes[0], flushes[1]);

    // Nothing after the second flush: it is the last thing a commit
    // does, and a write after it is a write outside the transaction.
    assert_eq!(
        second_flush,
        ops.len() - 1,
        "something was written after the commit point: {ops:?}"
    );

    // Every superblock write falls between the two flushes.
    let supers: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o, Op::Write { at, .. } if fs_btrfs::superblock::SUPER_OFFSETS.contains(at)))
        .map(|(i, _)| i)
        .collect();
    assert!(
        !supers.is_empty(),
        "no superblock was written, so nothing was committed: {ops:?}"
    );
    for i in &supers {
        assert!(
            *i > first_flush && *i < second_flush,
            "a superblock was written at step {i}, outside the two barriers. \
             Before the first, and it names blocks that may not be on the device; \
             after the second, and it is not covered by the commit point."
        );
    }

    // And every tree-block write falls before the first flush.
    let tree: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o, Op::Write { len, .. } if *len == nodesize))
        .map(|(i, _)| i)
        .collect();
    assert!(!tree.is_empty(), "no tree block was written: {ops:?}");
    for i in &tree {
        assert!(
            *i < first_flush,
            "a tree block was written at step {i}, after the barrier that is \
             supposed to order it against the superblock"
        );
    }

    eprintln!(
        "{} tree-block writes, barrier, {} superblock writes, barrier",
        tree.len(),
        supers.len()
    );
}

/// Every mirror of a block is written, not just the first.
///
/// On a `DUP` filesystem a block has two copies. Writing one leaves the
/// other holding what was there before, and the two then disagree with
/// nothing recording which is current — a later read may return either.
/// The trace shows the kernel writing both before the barrier.
#[test]
fn both_mirrors_of_a_dup_block_are_written() {
    let (fs, dev) = mounted("btrfs-dup.img");
    let nodesize = fs.superblock().nodesize as usize;

    let at = fs.find_metadata_block().expect("somewhere to put a block");
    let mirrors = fs
        .chunk_map()
        .mirrors_at(at)
        .expect("the address is inside a chunk");
    assert_eq!(
        mirrors, 2,
        "btrfs-dup.img is built with `-m dup`, so a metadata block there has \
         two copies; this fixture reports {mirrors}"
    );

    let bytes = vec![0x5A; nodesize];
    fs.commit(
        &[PlacedBlock {
            logical: at,
            bytes: bytes.clone(),
        }],
        &Commit {
            generation: fs.superblock().generation + 1,
            root: fs.superblock().root,
            ..Default::default()
        },
    )
    .expect("committing");

    // Both physical addresses must have received the bytes.
    for mirror in 0..mirrors {
        let m = fs
            .chunk_map()
            .map_mirror(at, mirror)
            .expect("mapping a mirror");
        let got = dev.writes_at(m.physical);
        assert!(
            !got.is_empty(),
            "mirror {mirror} of the block at {at} (physical {}) was never written. \
             A filesystem whose two copies disagree is worse than one with a single \
             copy: which one a read returns is not the caller's choice.",
            m.physical
        );
        assert_eq!(
            got[0], bytes,
            "mirror {mirror} received different bytes from mirror 0"
        );
    }
    eprintln!("{mirrors} mirrors written for one block");
}

/// Each superblock copy names its own address.
///
/// They are not the same image written three times: a copy records the
/// offset it belongs at, and a reader rejects one found somewhere else.
#[test]
fn each_superblock_copy_records_where_it_belongs() {
    let (fs, dev) = mounted("btrfs-default.img");
    fs.commit(
        &[],
        &Commit {
            generation: fs.superblock().generation + 1,
            root: fs.superblock().root,
            ..Default::default()
        },
    )
    .expect("committing");

    let mut checked = 0usize;
    for &offset in &fs_btrfs::superblock::SUPER_OFFSETS {
        for image in dev.writes_at(offset) {
            let bytenr = u64::from_le_bytes(image[0x30..0x38].try_into().unwrap());
            assert_eq!(
                bytenr, offset,
                "the copy written to {offset:#x} claims to belong at {bytenr:#x}"
            );
            // And its checksum must cover what it now says.
            assert!(
                fs.superblock()
                    .csum_type
                    .verify(&image[32..4096], &image[..32]),
                "the copy at {offset:#x} carries a checksum that does not cover it"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no superblock copy was written");
    eprintln!("{checked} superblock copies, each naming its own address");
}
