//! Every allocation record the kernel wrote, rebuilt from what a writer
//! would know.
//!
//! Handing out a free address is only half of allocating. Until the
//! extent tree says the address is taken, the next call returns it
//! again, and the second block lands on the first. The item that closes
//! that gap has a shape which depends on feature bits rather than on the
//! struct definition, so it is checked the same way the tree blocks are:
//! against the kernel's own.
//!
//! Each `METADATA_ITEM` on each fixture is rebuilt from only the four
//! things a writer has when it allocates — the address, the level, the
//! transaction, and the owning tree — and required to come back byte for
//! byte.
//!
//! The fixtures are gitignored and built by `chore fixtures`, in the
//! fs-linux-test-harness VM — every `METADATA_ITEM` rebuilt here is one
//! that kernel wrote. A missing fixture fails the test that wanted it:
//! an oracle with nothing to compare against passes without comparing.

use fs_btrfs::block_group::BlockGroup;
use fs_btrfs::chunk::{key_type, DiskKey};
use fs_btrfs::extent_write::{
    offsets, record_tree_block, used_after_allocating, TreeBlockAllocation,
    SKINNY_METADATA_ITEM_SIZE,
};
use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{fixture, fixtures_matching, le64, spans_several_devices};
use fs_core::FileDevice;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every fixture whose extent tree this file walks.
fn images() -> Vec<PathBuf> {
    let images: Vec<PathBuf> = fixtures_matching("btrfs-")
        .into_iter()
        // One member of a multi-device filesystem is refused a mount on
        // purpose: its extent tree accounts for space on the other
        // disk. `tests/pool_oracle.rs` is where such an image belongs.
        .filter(|p| !spans_several_devices(p))
        .collect();
    assert!(
        !images.is_empty(),
        "every fixture belongs to a multi-device filesystem, so there is no extent tree \
         here to rebuild from"
    );
    images
}

fn open(img: &Path) -> Filesystem {
    let dev = Arc::new(
        FileDevice::open(img).unwrap_or_else(|e| panic!("opening {}: {e}", img.display())),
    );
    Filesystem::mount(dev).unwrap_or_else(|e| panic!("mounting {}: {e}", img.display()))
}

/// Rebuilding an allocation record gives back what the kernel wrote.
#[test]
fn every_metadata_item_re_encodes_identically() {
    let images = images();

    let mut total = 0usize;
    let mut images_checked = 0usize;

    for img in &images {
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        let fs = open(img);
        let sb = fs.superblock().clone();

        let mut seen = 0usize;
        let mut failure: Option<String> = None;

        fs.for_each_extent_item(&mut |key: &DiskKey, data: &[u8]| {
            if key.key_type != key_type::METADATA_ITEM || failure.is_some() {
                return;
            }
            seen += 1;

            // Only what a writer would have: where the block went, how
            // tall it is, which transaction, and whose tree. Everything
            // else in the item has to come from the encoder, or the
            // comparison is circular.
            let alloc = TreeBlockAllocation {
                bytenr: key.objectid,
                level: key.offset as u8,
                generation: le64(data, offsets::GENERATION),
                owner: le64(data, offsets::REF_OFFSET),
            };

            let (ours_key, ours) = match record_tree_block(&sb, alloc) {
                Ok(v) => v,
                Err(e) => {
                    failure = Some(format!(
                        "the kernel wrote a METADATA_ITEM at {} and this refused to: {e}",
                        key.objectid
                    ));
                    return;
                }
            };

            if data.len() != SKINNY_METADATA_ITEM_SIZE {
                failure = Some(format!(
                    "the item at {} is {} bytes and this writes {SKINNY_METADATA_ITEM_SIZE}. \
                     A different length means a reference that is not inline, or a \
                     tree_block_info this does not write.",
                    key.objectid,
                    data.len()
                ));
                return;
            }

            if (ours_key.objectid, ours_key.key_type, ours_key.offset)
                != (key.objectid, key.key_type, key.offset)
            {
                failure = Some(format!(
                    "the key for the block at {} came back as {ours_key:?}, not {key:?}",
                    key.objectid
                ));
                return;
            }

            if let Some(i) = (0..SKINNY_METADATA_ITEM_SIZE).find(|&i| ours[i] != data[i]) {
                let field = match i {
                    0..=7 => "refs",
                    8..=15 => "generation",
                    16..=23 => "flags",
                    24 => "the inline reference's type",
                    _ => "the inline reference's offset",
                };
                failure = Some(format!(
                    "the item for the block at {} differs at byte {i} — {field} (ours \
                     {:#04x}, kernel {:#04x})",
                    key.objectid, ours[i], data[i]
                ));
            }
        })
        .unwrap_or_else(|e| panic!("{name}: walking the extent tree: {e}"));

        if let Some(msg) = failure {
            panic!("{name}: {msg}");
        }
        if seen > 0 {
            images_checked += 1;
            total += seen;
        }
    }

    assert!(
        images_checked > 0,
        "not one of {} fixtures had a METADATA_ITEM. Every filesystem's own trees are \
         made of tree blocks, so this is a read failure rather than an empty result.",
        images.len()
    );
    eprintln!("{total} allocation records rebuilt across {images_checked} images");
    assert!(
        total > 100,
        "only {total} records is too few to have exercised anything"
    );
}

/// Recording every block group's allocations arrives at the `used` count
/// the kernel wrote.
///
/// The encoder above proves an item is shaped right. This proves the
/// accounting that goes with it lands on the same number the kernel did
/// — starting from an empty group and adding each block back.
#[test]
fn replaying_the_allocations_reaches_the_used_count_the_kernel_recorded() {
    let mut checked = 0usize;
    for img in &images() {
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        let fs = open(img);
        let nodesize = fs.superblock().nodesize as u64;
        let groups = fs
            .block_groups()
            .unwrap_or_else(|e| panic!("{name}: reading the block groups: {e}"));

        // Every tree block, by which group holds it.
        let mut blocks: Vec<u64> = Vec::new();
        fs.for_each_extent_item(&mut |key: &DiskKey, _: &[u8]| {
            if key.key_type == key_type::METADATA_ITEM {
                blocks.push(key.objectid);
            }
        })
        .unwrap_or_else(|e| panic!("{name}: walking the extent tree: {e}"));

        for group in groups
            .iter()
            .filter(|g| g.holds_metadata() && !g.holds_data())
        {
            // Start from empty and add each block this group holds.
            let mut running = BlockGroup { used: 0, ..*group };
            for _ in blocks
                .iter()
                .filter(|&&b| b >= group.start && b < group.end())
            {
                running.used = used_after_allocating(&running, nodesize).unwrap_or_else(|e| {
                    panic!("{name}: replaying the group at {}: {e}", group.start)
                });
            }

            assert_eq!(
                running.used, group.used,
                "{name}: replaying every tree block in the group at {} reaches {} bytes \
                 used, but the kernel recorded {}",
                group.start, running.used, group.used
            );
            checked += 1;
        }
    }

    assert!(
        checked > 0,
        "no metadata-only block group was found to replay"
    );
    eprintln!("{checked} block groups replay to exactly the usage the kernel recorded");
}

/// A filesystem without `SKINNY_METADATA` is refused, not approximated.
#[test]
fn recording_is_refused_without_the_feature_that_defines_the_shape() {
    // Any fixture would do — the refusal is about a feature bit, not
    // about this image — so it names the plainest one.
    let fs = open(&fixture("btrfs-default.img"));

    let mut sb = fs.superblock().clone();
    sb.incompat_flags &= !fs_btrfs::superblock::incompat::SKINNY_METADATA;

    let err = record_tree_block(
        &sb,
        TreeBlockAllocation {
            bytenr: 4096,
            level: 0,
            generation: 1,
            owner: 5,
        },
    )
    .expect_err("without the feature the item is a different size");
    assert!(
        err.to_string().contains("SKINNY_METADATA"),
        "the refusal should name the feature it needs: {err}"
    );
}
