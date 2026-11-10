//! What is free, worked out twice and required to agree.
//!
//! Btrfs records allocation from both ends. The extent tree holds one
//! item per allocated run; the free-space tree holds the complement,
//! maintained separately by the kernel. Deriving free space from the
//! first and comparing it against the second is a check neither source
//! can make of itself — they are different items, written at different
//! times, by different code.
//!
//! It is also the check that catches the mistake this is most likely to
//! make. Under `SKINNY_METADATA` a tree block is recorded as a
//! `METADATA_ITEM` whose key offset is the block's LEVEL, not a length.
//! Read as a length it gives extents of 0, 1 and 2 bytes, and every tree
//! block on the filesystem reads as free — an allocator would then hand
//! out the address of the root tree. Against the kernel's own free-space
//! tree that is not a subtle discrepancy; it is thousands of them.
//!
//! The fixtures are gitignored and built by `chore fixtures`, in the
//! fs-linux-test-harness VM — the free-space tree compared against here
//! is one the kernel maintained. A fixture that is not there fails the
//! test that wanted it; nothing below returns early.

use fs_btrfs::block_group::{BlockGroup, FreeExtent};
use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{fixtures_matching, spans_several_devices};
use fs_core::FileDevice;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every fixture this file compares, which is every one that mounts.
fn images() -> Vec<PathBuf> {
    let images: Vec<PathBuf> = fixtures_matching("btrfs-")
        .into_iter()
        // One member of a multi-device filesystem is refused a mount on
        // purpose: its block groups describe space on the other disk.
        // The pool is `tests/pool_oracle.rs`'s subject, not this file's.
        .filter(|p| !spans_several_devices(p))
        .collect();
    assert!(
        !images.is_empty(),
        "every fixture belongs to a multi-device filesystem, so there is nothing here \
         this file can mount and account for"
    );
    images
}

fn open(img: &Path) -> Filesystem {
    let dev = Arc::new(
        FileDevice::open(img).unwrap_or_else(|e| panic!("opening {}: {e}", img.display())),
    );
    Filesystem::mount(dev).unwrap_or_else(|e| panic!("mounting {}: {e}", img.display()))
}

/// Every block group of an image, or a failure naming the image.
///
/// A filesystem's own root tree has to live in a block group, so a read
/// that comes back empty or in error is this crate's failure rather
/// than a quiet reason to move to the next fixture.
fn block_groups(fs: &Filesystem, name: &str) -> Vec<BlockGroup> {
    let groups = fs
        .block_groups()
        .unwrap_or_else(|e| panic!("{name}: reading the block groups: {e}"));
    assert!(
        !groups.is_empty(),
        "{name}: no block group at all — the filesystem's own root tree has to live \
         somewhere, so this is a read failure, not an empty filesystem"
    );
    groups
}

/// A short description of where two free-space lists first diverge.
///
/// Printing the lists themselves is useless — a metadata group has
/// hundreds of runs. The first disagreement is the whole diagnosis.
fn first_difference(ours: &[FreeExtent], theirs: &[FreeExtent]) -> Option<String> {
    for (i, (a, b)) in ours.iter().zip(theirs.iter()).enumerate() {
        if a != b {
            return Some(format!(
                "run {i}: the extent tree says {}..{} ({} bytes), the free-space tree says \
                 {}..{} ({} bytes)",
                a.start,
                a.end(),
                a.len,
                b.start,
                b.end(),
                b.len
            ));
        }
    }
    if ours.len() != theirs.len() {
        let (longer, which) = if ours.len() > theirs.len() {
            (&ours[theirs.len()], "extent tree")
        } else {
            (&theirs[ours.len()], "free-space tree")
        };
        return Some(format!(
            "the lists agree for {} runs, then the {which} has {}..{} and the other has \
             nothing ({} runs vs {})",
            ours.len().min(theirs.len()),
            longer.start,
            longer.end(),
            ours.len(),
            theirs.len()
        ));
    }
    None
}

/// The two records of allocation describe the same filesystem.
#[test]
fn free_space_derived_from_the_extent_tree_matches_the_kernels_cache() {
    let images = images();

    let mut groups_checked = 0usize;
    let mut images_with_cache = 0usize;
    let mut runs = 0usize;

    for img in &images {
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        let fs = open(img);
        let groups = block_groups(&fs, &name);

        let mut had_cache = false;
        // One traversal for every group, not one per group.
        let derived = fs
            .free_extents_by_group(&groups)
            .unwrap_or_else(|e| panic!("{name}: deriving free space: {e}"));
        for (group, ours) in groups.iter().zip(derived) {
            let theirs = match fs.cached_free_extents(group) {
                Ok(Some(t)) => t,
                // No free-space tree on this filesystem: nothing to
                // compare against, and not a failure.
                Ok(None) => continue,
                Err(e) => panic!(
                    "{name}: reading the free-space tree at {}: {e}",
                    group.start
                ),
            };
            had_cache = true;

            if let Some(diff) = first_difference(&ours, &theirs) {
                panic!(
                    "{name}: block group at {} (flags {:#x}, {} of {} bytes used) — the two \
                     records of what is free disagree. {diff}",
                    group.start, group.flags, group.used, group.length
                );
            }
            groups_checked += 1;
            runs += ours.len();
        }
        if had_cache {
            images_with_cache += 1;
        }
    }

    assert!(
        images_with_cache > 0,
        "not one of {} fixtures had a free-space tree, so the comparison never happened. \
         mkfs.btrfs has enabled it by default for years, so this is a fixture problem.",
        images.len()
    );
    eprintln!(
        "{groups_checked} block groups across {images_with_cache} images: {runs} free runs, \
         derived from the extent tree and confirmed against the kernel's free-space tree"
    );
}

/// Each group's `used` is what its allocated extents actually occupy.
///
/// A separate claim from the one above and a stricter one in a
/// particular way: free-space lists could agree while both were shifted,
/// but the used total is an independent number the kernel wrote into the
/// block group item.
#[test]
fn each_groups_used_count_matches_what_is_allocated_in_it() {
    let mut checked = 0usize;
    for img in &images() {
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        let fs = open(img);

        for group in &block_groups(&fs, &name) {
            let free = fs.free_extents(group).unwrap_or_else(|e| {
                panic!("{name}: deriving the free space at {}: {e}", group.start)
            });
            let free_bytes: u64 = free.iter().map(|r| r.len).sum();
            let allocated = group.length - free_bytes;

            assert_eq!(
                allocated, group.used,
                "{name}: the block group at {} says {} bytes are used, but the extent tree \
                 accounts for {allocated}. A difference of one nodesize means a metadata \
                 item was read as an extent length, or not read at all.",
                group.start, group.used
            );
            checked += 1;
        }
    }

    assert!(checked > 0, "no block group was checked");
    eprintln!("{checked} block groups account for exactly the bytes they say are used");
}

/// The superblock's `bytes_used` is the sum over every group.
///
/// The superblock writer takes this number on trust from its caller, so
/// this is where the rule it documents gets checked.
#[test]
fn the_superblock_total_is_the_sum_of_every_group() {
    let mut checked = 0usize;
    for img in &images() {
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        let fs = open(img);
        let groups = block_groups(&fs, &name);
        let total: u64 = groups.iter().map(|g| g.used).sum();
        assert_eq!(
            total,
            fs.superblock().bytes_used,
            "{name}: the superblock says {} bytes are used and the {} block groups sum to \
             {total}",
            fs.superblock().bytes_used,
            groups.len()
        );
        checked += 1;
    }
    assert!(checked > 0, "no image was checked");
    eprintln!("{checked} superblocks agree with the sum of their block groups");
}

/// The address the allocator picks is genuinely free and correctly
/// aligned.
///
/// Checked against the kernel's free-space tree rather than against the
/// derivation that produced it, so a wrong derivation cannot certify its
/// own answer.
#[test]
fn the_address_the_allocator_picks_is_free_in_the_kernels_own_record() {
    let mut checked = 0usize;
    for img in &images() {
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        let fs = open(img);
        let nodesize = fs.superblock().nodesize as u64;

        // Every fixture has room for one more tree block, so a refusal
        // here is either a full image — a fixture the builder should
        // not have produced — or the allocator failing to see space
        // that is there. Both are findings; neither is a reason to move
        // on to the next image.
        let at = fs
            .find_metadata_block()
            .unwrap_or_else(|e| panic!("{name}: the allocator found nowhere to put a block: {e}"));

        assert_eq!(
            at % nodesize,
            0,
            "{name}: {at} is not aligned to a {nodesize}-byte tree block"
        );

        let groups = block_groups(&fs, &name);
        let group: &BlockGroup = groups
            .iter()
            .find(|g| at >= g.start && at < g.end())
            .unwrap_or_else(|| panic!("{name}: {at} is in no block group at all"));

        assert!(
            group.holds_metadata(),
            "{name}: {at} is in the group at {} which has flags {:#x} and does not take \
             metadata",
            group.start,
            group.flags
        );

        // A filesystem without a free-space tree has no second record
        // to check the pick against, which is a property of the image
        // rather than a result. The count below is what keeps that from
        // silently emptying the test.
        let cached = match fs.cached_free_extents(group) {
            Ok(Some(cached)) => cached,
            Ok(None) => continue,
            Err(e) => panic!(
                "{name}: reading the free-space tree at {}: {e}",
                group.start
            ),
        };
        let covered = cached
            .iter()
            .any(|r| at >= r.start && at + nodesize <= r.end());
        assert!(
            covered,
            "{name}: the allocator picked {at}, but the kernel's free-space tree does not \
             show {nodesize} free bytes there. Writing a tree block at an allocated \
             address is the failure this test exists to prevent."
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "not one fixture had a free-space tree covering the address the allocator picked, \
         so the pick was never confirmed against the kernel's own record"
    );
    eprintln!("{checked} allocations land on space the kernel also considers free");
}
