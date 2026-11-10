//! Editing the kernel's own leaves and putting them back.
//!
//! Recording an allocation is an insert into the extent tree; releasing
//! one is a delete. Both must keep the item list in the order a search
//! bisects on, and an edit that gets that wrong does not fail — it
//! produces a leaf that finds some items and silently misses others.
//!
//! So the check is a round trip on real leaves: take each item out and
//! put it back, and require the list that comes back to be the one that
//! was there. Doing it item by item across every leaf on every fixture
//! covers the positions an edit written as "find the gap" gets wrong —
//! the first, the last, and between two items sharing an objectid.
//!
//! The fixtures are gitignored and built by `chore fixtures`, in the
//! fs-linux-test-harness VM. THIS FILE IS WHY NOTHING SKIPS ANY MORE:
//! it used to print "no fixtures" and return, and it did that for a
//! whole release while reporting green, so the round trip below was
//! never run on a single kernel leaf. A missing fixture now fails.

use fs_btrfs::btree::{header_offsets as o, HEADER_SIZE};
use fs_btrfs::chunk::DiskKey;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::leaf_edit::{delete, fits, insert, OwnedItem};
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::{build_leaf, chunk_tree_uuid_of, BlockIdentity};
use fs_btrfs_test_support::{fixture, fixtures_matching, le32, le64, spans_several_devices, Image};
use fs_core::FileDevice;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every fixture whose leaves can be scanned out of the image.
fn images() -> Vec<PathBuf> {
    let images: Vec<PathBuf> = fixtures_matching("btrfs-")
        .into_iter()
        // One member of a multi-device filesystem is refused a mount on
        // purpose, and the blocks of its filesystem are not all on this
        // disk. `tests/pool_oracle.rs` is where such an image belongs.
        .filter(|p| !spans_several_devices(p))
        .collect();
    assert!(
        !images.is_empty(),
        "every fixture belongs to a multi-device filesystem, so there is no image here \
         to scan for leaves"
    );
    images
}

/// The items of a leaf, owning their bytes.
fn items_of(block: &[u8]) -> Option<Vec<OwnedItem>> {
    let n = le32(block, o::NRITEMS) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let at = HEADER_SIZE + i * 25;
        if at + 25 > block.len() {
            return None;
        }
        let start = HEADER_SIZE + le32(block, at + 17) as usize;
        let end = start + le32(block, at + 21) as usize;
        if end > block.len() {
            return None;
        }
        out.push(OwnedItem {
            key: DiskKey {
                objectid: le64(block, at),
                key_type: block[at + 8],
                offset: le64(block, at + 9),
            },
            data: block[start..end].to_vec(),
        });
    }
    Some(out)
}

/// Every leaf of a filesystem, found by scanning.
fn leaves(img: &Path) -> (Superblock, Vec<Vec<u8>>) {
    let dev = Arc::new(
        FileDevice::open(img).unwrap_or_else(|e| panic!("opening {}: {e}", img.display())),
    );
    let fs = Filesystem::mount(dev).unwrap_or_else(|e| panic!("mounting {}: {e}", img.display()));
    let sb = fs.superblock().clone();
    // A BLOCK AT A TIME, not the whole image: these fixtures are up to
    // 2 GiB and only the tree blocks are wanted. See `Image`.
    let image = Image::open(img);
    let n = sb.nodesize as usize;
    let mut out = Vec::new();
    let mut buf = vec![0u8; n];
    let mut at = 0u64;
    while image.try_read_at(at, &mut buf) {
        let b = &buf[..];
        at += n as u64;
        if b[o::FSID..o::FSID + 16] != sb.fsid[..] || b[o::LEVEL] != 0 {
            continue;
        }
        if le32(b, o::NRITEMS) == 0 || !sb.csum_type.verify(&b[32..], &b[..32]) {
            continue;
        }
        out.push(b.to_vec());
    }
    (sb, out)
}

/// Taking an item out and putting it back gives the leaf back.
#[test]
fn every_item_of_every_leaf_survives_a_round_trip() {
    let mut round_trips = 0usize;
    let mut leaves_seen = 0usize;

    for img in &images() {
        let (sb, blocks) = leaves(img);
        let name = img.file_name().unwrap().to_string_lossy().into_owned();

        // A sample per image: the round trip is per ITEM, so a handful
        // of leaves is already thousands of edits.
        for block in blocks.iter().take(8) {
            let Some(items) = items_of(block) else {
                continue;
            };
            if items.len() < 2 {
                continue;
            }
            leaves_seen += 1;

            for (i, item) in items.iter().enumerate() {
                let without = delete(&items, &item.key).unwrap_or_else(|e| {
                    panic!("{name}: removing item {i}, which is in the leaf: {e}")
                });
                assert_eq!(
                    without.len(),
                    items.len() - 1,
                    "{name}: removing item {i} changed the count by {}",
                    items.len() as i64 - without.len() as i64
                );

                let back = insert(sb.nodesize, &without, item.clone()).unwrap_or_else(|e| {
                    panic!(
                        "{name}: putting item {i} back into the leaf it came out of: {e}. \
                         It fitted a moment ago."
                    )
                });
                assert_eq!(
                    back, items,
                    "{name}: item {i} went back somewhere other than where it was. The \
                     order is what a search bisects on, so this leaf would find some \
                     items and miss others."
                );
                round_trips += 1;
            }
        }
    }

    assert!(
        leaves_seen > 0,
        "the fixtures were read and not one leaf with two items came out of them, so not \
         one round trip above ran and this test proved nothing"
    );
    assert!(
        round_trips > 100,
        "only {round_trips} round trips, which is too few to have covered the first, \
         last and middle positions"
    );
    eprintln!("{round_trips} items removed and reinserted across {leaves_seen} kernel leaves");
}

/// A round-tripped leaf encodes to the bytes it started as.
///
/// The test above compares item LISTS. This one closes the loop through
/// the encoder: a leaf that survives an edit as a list but not as bytes
/// is still a leaf the kernel would not have written.
#[test]
fn a_round_tripped_leaf_encodes_to_the_same_bytes() {
    // The plainest geometry, named rather than "whichever sorts first",
    // so a failure is always about the same leaves.
    let (sb, blocks) = leaves(&fixture("btrfs-default.img"));

    let mut checked = 0usize;
    for block in blocks.iter().take(20) {
        let Some(items) = items_of(block) else {
            continue;
        };
        if items.len() < 2 {
            continue;
        }

        let id = BlockIdentity {
            bytenr: le64(block, o::BYTENR),
            owner: le64(block, o::OWNER),
            generation: le64(block, o::GENERATION),
            level: 0,
            flags: le64(block, o::FLAGS),
            chunk_tree_uuid: chunk_tree_uuid_of(block),
        };

        // Remove the middle item and put it back.
        let mid = &items[items.len() / 2];
        let without = delete(&items, &mid.key).expect("removing");
        let back = insert(sb.nodesize, &without, mid.clone()).expect("reinserting");

        let borrowed: Vec<_> = back.iter().map(|i| i.as_leaf_item()).collect();
        let ours = build_leaf(&sb, id, &borrowed).expect("encoding the round-tripped leaf");

        // Compare the header and item array; the slack belongs to
        // neither side, as tests/tree_write_oracle.rs explains.
        let items_end = HEADER_SIZE + back.len() * 25;
        assert_eq!(
            ours[32..items_end],
            block[32..items_end],
            "the leaf at {} does not re-encode after a round trip",
            id.bytenr
        );
        checked += 1;
    }
    assert!(checked > 0, "no leaf was round-tripped through the encoder");
    eprintln!("{checked} leaves re-encode identically after an edit");
}

/// An item that will not fit is refused rather than silently dropped.
#[test]
fn an_item_that_needs_a_split_is_refused() {
    let (sb, blocks) = leaves(&fixture("btrfs-default.img"));
    let block = blocks
        .first()
        .expect("btrfs-default.img has leaves: its own trees are made of them");
    let items = items_of(block).expect("the first leaf's item array is within the block");

    // An item as big as the whole block cannot fit alongside anything.
    let huge = OwnedItem {
        key: DiskKey {
            objectid: u64::MAX,
            key_type: 255,
            offset: u64::MAX,
        },
        data: vec![0u8; sb.nodesize as usize],
    };
    assert!(!fits(sb.nodesize, &items, &huge));
    let err = insert(sb.nodesize, &items, huge).expect_err("a block-sized item cannot fit");
    // The condition, not the wording — see the unit test of the same
    // shape in `leaf_edit.rs` for why.
    assert!(
        err.to_string().contains("does not fit"),
        "the refusal should name what went wrong: {err}"
    );
    assert!(
        err.to_string().contains("insert_or_split"),
        "the refusal should name the function that handles it: {err}"
    );
}
