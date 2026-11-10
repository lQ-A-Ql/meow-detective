//! What must be true of a transaction, whatever it was for.
//!
//! The remaining piece of the write path decides which blocks a change
//! produces. That decision is recursive — recording an allocation
//! modifies the extent tree, which lives in blocks that must themselves
//! be allocated — and it is the part where a plausible implementation
//! can be wrong in ways that still mount.
//!
//! So the checks come first. These are asserted against the KERNEL's
//! transactions, on before/after image pairs it produced, and they are
//! written before the planner exists precisely so the planner has
//! something to satisfy that was not shaped around it. When it lands,
//! its output goes through the same three assertions.
//!
//! `docs/cow-transaction.md` records what the pairs showed. The short
//! version: an empty commit is not empty — it rewrites the root, extent,
//! free-space and dev trees, four blocks in and four out — and a `touch`
//! costs that floor plus two fs-tree blocks.
//!
//! # Both geometries, every run
//!
//! The pairs exist twice: on a default `mkfs.btrfs`, and on a volume
//! made with `--csum sha256 -d dup -m dup`, which is a wider checksum
//! and a duplicated allocation profile. Which of the two was checked
//! used to depend on `BTRFS_COW_SUFFIX` being set by the workflow that
//! ran the suite, so the DUP geometry was covered only where somebody
//! had remembered to ask for it. Both are required here and both are
//! checked, and every failure says which one it came from.
//!
//! The fixtures are gitignored and built by `chore fixtures`.

use fs_btrfs::btree::{header_offsets as o, HEADER_SIZE};
use fs_btrfs::chunk::{key_type, DiskKey};
use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{fixture, le16, le64, temp_path};
use fs_core::FileDevice;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The two geometries the COW fixtures are built on: how a failure names
/// one, and the suffix its images carry.
const GEOMETRIES: [(&str, &str); 2] = [("default geometry", ""), ("sha256+dup", "-sha256-dup")];

fn mount(p: &Path) -> Filesystem {
    let dev = Arc::new(FileDevice::open(p).expect("opening a fixture"));
    Filesystem::mount(dev).expect("mounting a fixture")
}

/// The addresses named by a `METADATA_ITEM`, i.e. the tree blocks the
/// filesystem believes are allocated.
fn recorded_blocks(fs: &Filesystem) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    fs.for_each_extent_item(&mut |k: &DiskKey, _: &[u8]| {
        if k.key_type == key_type::METADATA_ITEM {
            out.insert(k.objectid);
        }
    })
    .expect("walking the extent tree");
    out
}

/// Every tree block present in an image whose checksum verifies, as
/// (address, generation, owner).
///
/// Found by scanning rather than walking, so it reaches blocks the
/// current root no longer points at — which is exactly the set a
/// transaction leaves behind.
///
/// Deduplicated by logical address, because the scan is physical and a
/// DUP filesystem stores every block twice. Counting both copies is not
/// wrong so much as a different question: four blocks written to a
/// mirrored filesystem is eight writes and four blocks, and it is the
/// blocks that a transaction is measured in.
fn present_blocks(fs: &Filesystem, path: &Path) -> BTreeSet<(u64, u64, u64)> {
    let sb = fs.superblock();
    let bytes = std::fs::read(path).expect("reading a fixture");
    let n = sb.nodesize as usize;
    let mut out = BTreeSet::new();
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
        out.insert((
            le64(b, o::BYTENR),
            le64(b, o::GENERATION),
            le64(b, o::OWNER),
        ));
    }
    out
}

/// Whether a pair actually contains a transaction.
///
/// A mount cycle that changes nothing DOES NOT ALWAYS COMMIT. It did in
/// the Debian oracle VM, where the control pair was measured, and it did
/// not on the CI runner — same script, same fixture, generation
/// unmoved. Which is fair: there was nothing to write.
///
/// So a pair with no transaction in it is not a failure, it is a pair
/// with nothing to check. The test below passes over those AND requires
/// that something, somewhere, did commit — otherwise a fixture builder
/// that silently stopped producing transactions would read as a pass.
fn committed(before: &Path, after: &Path) -> bool {
    mount(after).superblock().generation > mount(before).superblock().generation
}

/// Every pair, on every geometry: the empty commit and the one touch,
/// each named so a failure says which volume it happened on.
fn pairs() -> Vec<(String, PathBuf, PathBuf)> {
    let mut out = Vec::new();
    for (label, suffix) in GEOMETRIES {
        let before = fixture(&format!("btrfs-cow-before{suffix}.img"));
        let control = fixture(&format!("btrfs-cow-control{suffix}.img"));
        let after = fixture(&format!("btrfs-cow-after{suffix}.img"));
        out.push((format!("an empty commit, {label}"), before.clone(), control));
        out.push((format!("one touch, {label}"), before, after));
    }
    out
}

/// Every block the filesystem can reach is recorded as allocated.
///
/// A block that is reachable but unrecorded is one the allocator will
/// hand out again. The filesystem will still mount, still read
/// correctly, and corrupt itself on the next transaction — which makes
/// this the invariant that gives allocation its meaning.
///
/// # Reachability, not generation
///
/// The obvious cheap test — every block stamped with the current
/// generation must be recorded — is WRONG, and it took two CI runs to
/// stop being wrong in a new way each time. A block written by a
/// transaction can be superseded within the sequence a fixture pair
/// spans, and is then still on the disk, still checksumming, and
/// correctly unrecorded. Generation says when a block was written. It
/// does not say whether anything still points at it.
///
/// So this descends: from the root tree, through every `ROOT_ITEM` it
/// holds, through every node to every leaf. What that reaches is what
/// the filesystem is. Anything else on the disk is debris.
fn reachable(fs: &Filesystem, image: &[u8]) -> BTreeSet<u64> {
    let sb = fs.superblock();
    let nodesize = sb.nodesize as usize;

    // Read one block by logical address, through the chunk map, from
    // the image already in memory.
    let read = |logical: u64| -> Option<Vec<u8>> {
        let m = fs.chunk_map().map(logical).ok()?;
        let at = m.physical as usize;
        let end = at.checked_add(nodesize)?;
        if end > image.len() {
            return None;
        }
        let b = image[at..end].to_vec();
        sb.csum_type.verify(&b[32..], &b[..32]).then_some(b)
    };

    /// `btrfs_root_item.bytenr` — the address of that tree's root.
    const ROOT_ITEM_BYTENR: usize = 176;
    const ROOT_ITEM_KEY: u8 = 132;

    let mut seen = BTreeSet::new();
    // The chunk tree is reachable from the superblock rather than from
    // the root tree, being what makes reading the root tree possible.
    let mut stack = vec![sb.root, sb.chunk_root];

    while let Some(at) = stack.pop() {
        if at == 0 || !seen.insert(at) {
            continue;
        }
        let Some(b) = read(at) else { continue };
        let nritems =
            u32::from_le_bytes(b[o::NRITEMS..o::NRITEMS + 4].try_into().unwrap()) as usize;

        if b[o::LEVEL] > 0 {
            // A node: 33-byte key pointers, the address at offset 17.
            for i in 0..nritems {
                let p = HEADER_SIZE + i * 33;
                if p + 25 <= b.len() {
                    stack.push(le64(&b, p + 17));
                }
            }
            continue;
        }

        // A leaf. Only ROOT_ITEMs lead anywhere: they name the roots of
        // the other trees.
        for i in 0..nritems {
            let it = HEADER_SIZE + i * 25;
            if it + 25 > b.len() || b[it + 8] != ROOT_ITEM_KEY {
                continue;
            }
            let off =
                HEADER_SIZE + u32::from_le_bytes(b[it + 17..it + 21].try_into().unwrap()) as usize;
            if off + ROOT_ITEM_BYTENR + 8 <= b.len() {
                stack.push(le64(&b, off + ROOT_ITEM_BYTENR));
            }
        }
    }
    seen
}

/// Nothing the filesystem points at is missing from the extent tree.
#[test]
fn every_reachable_block_is_recorded_as_allocated() {
    let pairs = pairs();

    let mut checked = 0usize;
    for (what, before, after) in &pairs {
        for (side, path) in [("before", before), ("after", after)] {
            let fs = mount(path);
            let image = std::fs::read(path).expect("reading a fixture");
            let recorded = recorded_blocks(&fs);
            let live = reachable(&fs, &image);

            assert!(
                live.len() > 1,
                "{what} ({side}): only {} block reachable, so the descent found nothing \
                 and this test proves nothing",
                live.len()
            );

            for at in &live {
                assert!(
                    recorded.contains(at),
                    "{what} ({side}): the block at {at} is reachable from the superblock \
                     but has no METADATA_ITEM. The allocator will hand that address out \
                     again, and the filesystem will still mount until it does."
                );
            }
            checked += live.len();
        }
        eprintln!("{what}: reachable blocks all recorded, both sides");
    }
    eprintln!("{checked} reachable blocks checked");
}

/// The superblock's total is the sum of the block groups, after as
/// before.
///
/// Checked on both sides of every pair rather than only the result: a
/// transaction that corrupts this leaves a filesystem whose own two
/// accounts of its size disagree.
#[test]
fn usage_adds_up_on_both_sides_of_every_transaction() {
    let pairs = pairs();

    let mut checked = 0usize;
    for (what, before, after) in &pairs {
        for (side, path) in [("before", before), ("after", after)] {
            let fs = mount(path);
            let groups = fs.block_groups().expect("reading block groups");
            let total: u64 = groups.iter().map(|g| g.used).sum();
            assert_eq!(
                total,
                fs.superblock().bytes_used,
                "{what} ({side}): the superblock says {} bytes are used and the {} block \
                 groups sum to {total}",
                fs.superblock().bytes_used,
                groups.len()
            );
            checked += 1;
        }
    }
    eprintln!("{checked} images account for themselves");
}

/// What `bytes_used` moved by is what the allocation records moved by.
///
/// The general form of an observation that looks like a coincidence on
/// these fixtures — `bytes_used` did not change across either
/// transaction — because four blocks were allocated and four released.
/// Stated as a delta it still holds when a tree grows, which is the case
/// the fixtures do not cover and a planner will eventually produce.
#[test]
fn the_change_in_usage_is_the_change_in_recorded_blocks() {
    let pairs = pairs();

    let mut committed_any = false;
    for (what, before, after) in &pairs {
        let fs_before = mount(before);
        let fs_after = mount(after);
        let nodesize = fs_after.superblock().nodesize as i128;

        let was = recorded_blocks(&fs_before);
        let now = recorded_blocks(&fs_after);
        let added = now.difference(&was).count() as i128;
        let removed = was.difference(&now).count() as i128;

        if !committed(before, after) {
            eprintln!("{what}: no commit happened on this run — nothing to check");
            continue;
        }
        committed_any = true;
        let used_delta =
            fs_after.superblock().bytes_used as i128 - fs_before.superblock().bytes_used as i128;
        let record_delta = (added - removed) * nodesize;

        assert_eq!(
            used_delta, record_delta,
            "{what}: bytes_used moved by {used_delta} but the allocation records moved by \
             {record_delta} ({added} added, {removed} removed, nodesize {nodesize}). \
             Those are two accounts of the same thing."
        );
        eprintln!("{what}: {added} recorded, {removed} released, bytes_used {used_delta:+}");
    }

    assert!(
        committed_any,
        "not one of the {} pairs holds a transaction, so this compared nothing. A fixture \
         builder that stopped producing commits looks exactly like this.",
        pairs.len()
    );
}

/// Every block recorded as allocated is really there.
///
/// The converse of the above, and the more dangerous direction. A
/// `METADATA_ITEM` naming an address nothing was written to is a
/// filesystem that believes a tree block exists where there is
/// whatever was there before. It will not be caught by mounting, or by
/// reading the files — only by descending into that particular block,
/// which may be years later.
///
/// A writer gets this wrong by recording an allocation and then failing
/// to write the block, or by writing it somewhere else.
#[test]
fn every_block_recorded_as_allocated_is_really_on_the_disk() {
    let pairs = pairs();

    let mut checked = 0usize;
    for (what, before, after) in &pairs {
        for (side, path) in [("before", before), ("after", after)] {
            let fs = mount(path);
            let present: BTreeSet<u64> = present_blocks(&fs, path)
                .into_iter()
                .map(|(bytenr, _, _)| bytenr)
                .collect();

            for bytenr in recorded_blocks(&fs) {
                assert!(
                    present.contains(&bytenr),
                    "{what} ({side}): a METADATA_ITEM names {bytenr}, but no tree block \
                     with that address and a valid checksum is on the disk. The \
                     filesystem believes there is a tree block where there is not."
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no recorded block was checked");
    eprintln!("{checked} recorded blocks are all physically present");
}

/// The accounting check is not vacuous.
///
/// Every assertion above passes on every fixture, which is what it looks
/// like when a check is right and also what it looks like when a check
/// cannot fail. The kernel's images cannot be made to violate an
/// invariant on request, so one is broken deliberately here: a copy of a
/// fixture with `bytes_used` moved by one block, re-checksummed so it
/// still mounts.
///
/// If this does not fail, the sum check is decoration.
#[test]
fn an_image_whose_accounts_disagree_is_caught() {
    for (label, suffix) in GEOMETRIES {
        accounts_that_disagree_are_caught(label, suffix);
    }
}

fn accounts_that_disagree_are_caught(label: &str, suffix: &str) {
    let path = fixture(&format!("btrfs-cow-before{suffix}.img"));

    let mut bytes = std::fs::read(&path).expect("reading a fixture");
    let sb_at = fs_btrfs::superblock::SUPER_OFFSETS[0] as usize;
    let mut raw = bytes[sb_at..sb_at + 4096].to_vec();

    let nodesize = {
        let fs = mount(&path);
        fs.superblock().nodesize as u64
    };

    // One block's worth of usage that no block group accounts for.
    let at = fs_btrfs::super_write::offsets::BYTES_USED;
    let was = le64(&raw, at);
    raw[at..at + 8].copy_from_slice(&(was + nodesize).to_le_bytes());
    // Re-checksummed, or it would not mount and this would prove
    // nothing about the check under test.
    let csum_type = fs_btrfs::superblock::ChecksumType::from_raw(le16(
        &raw,
        fs_btrfs::superblock::offsets::CSUM_TYPE,
    ))
    .expect("captured superblock names a supported checksum");
    fs_btrfs::super_write::stamp_checksum(&mut raw, csum_type);
    bytes[sb_at..sb_at + 4096].copy_from_slice(&raw);

    // Inside the repository, like every scratch file this suite writes:
    // the guest that runs the oracle tools sees this tree and nothing
    // else of the host.
    let broken = PathBuf::from(temp_path!("cow-accounts-disagree{suffix}.img"));
    std::fs::write(&broken, &bytes).expect("writing the broken copy");

    let fs = mount(&broken);
    assert_eq!(
        fs.superblock().bytes_used,
        was + nodesize,
        "[{label}] the edit did not take, so nothing was tested"
    );

    let groups = fs.block_groups().expect("reading block groups");
    let total: u64 = groups.iter().map(|g| g.used).sum();
    assert_ne!(
        total,
        fs.superblock().bytes_used,
        "[{label}] an image edited to disagree with itself still adds up, so the check \
         that compares them cannot fail"
    );

    let _ = std::fs::remove_file(&broken);
    eprintln!(
        "[{label}] a superblock claiming {} against block groups summing to {total} is \
         detectable",
        fs.superblock().bytes_used
    );
}
