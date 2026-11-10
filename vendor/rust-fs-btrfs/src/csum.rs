//! The checksum tree: what the filesystem says its data should hash to.
//!
//! Btrfs checksums data as well as metadata. Every sector of every
//! ordinary data extent has a digest filed in a tree of its own, keyed
//! by the sector's logical address, and the kernel verifies each one on
//! the way out. A driver that skips the check hands back bit-rotted or
//! clobbered bytes with a successful return — the one failure a caller
//! cannot detect, and the one this crate's module docs say it exists to
//! prevent.
//!
//! ## How the tree is keyed
//!
//! One objectid, one type, and the logical address in the offset:
//!
//! ```text
//! objectid = BTRFS_EXTENT_CSUM_OBJECTID  (-10, i.e. u64::MAX - 9)
//! type     = BTRFS_EXTENT_CSUM_KEY       (128)
//! offset   = logical address of the first sector the item covers
//! ```
//!
//! An item's data is a packed array of digests, `csum_size` bytes each,
//! covering consecutive sectors from `offset` onwards. So one item can
//! describe a long run, and the item covering a given sector is the
//! **last** one whose offset is at or below it — not necessarily one
//! filed at exactly that address. Looking up an exact key finds a
//! checksum only when the sector happens to start a run, which is the
//! easy mistake to make here and would leave most sectors silently
//! unchecked.
//!
//! ## What absence means
//!
//! Not every sector has a digest, and a missing one is not a fault:
//!
//! - a file marked `NODATASUM` has none by definition;
//! - `NODATACOW` implies `NODATASUM`, because a block rewritten in
//!   place cannot keep a checksum consistent with itself;
//! - a preallocated extent holds no data yet.
//!
//! So absence is "nothing to check", and only a digest that is present
//! and does not match is an error. That asymmetry is the whole policy:
//! the tree is authoritative about the sectors it names and silent
//! about the rest.

use std::collections::BTreeMap;

use crate::btree::Tree;
use crate::chunk::DiskKey;
use crate::error::Result;

/// `BTRFS_CSUM_TREE_OBJECTID` — the root tree's entry for this tree.
pub const CSUM_TREE_OBJECTID: u64 = 7;

/// `BTRFS_EXTENT_CSUM_KEY`.
pub const EXTENT_CSUM_KEY: u8 = 128;

/// `BTRFS_EXTENT_CSUM_OBJECTID`, which the kernel spells `-10`.
///
/// Keys are compared as unsigned, so this sorts near the top of the
/// tree; it is a constant tag rather than a number with arithmetic
/// meaning.
pub const EXTENT_CSUM_OBJECTID: u64 = u64::MAX - 9;

/// The digests covering a run of sectors, by the logical address of the
/// sector each one describes.
///
/// Only sectors the tree actually names are present. See the module doc
/// for why a missing sector is not a failure.
pub type SectorDigests = BTreeMap<u64, Vec<u8>>;

/// Every digest the csum tree holds for the sectors in
/// `[start, start + len)`.
///
/// `start` and `len` are expected to be sector-aligned; a caller reading
/// a sub-range widens it to sector boundaries first, because a digest
/// covers a whole sector and there is nothing to compare a fragment
/// against.
///
/// The scan begins at the item covering `start` rather than at `start`
/// itself. A `for_each_from` seek lands on the first key at or after
/// its target, so seeking to `start` steps *over* the item that covers
/// it whenever that item begins earlier — which is the common case, as
/// one item covers a long run. The leaf holding the seek point is
/// examined directly to find the item at or before `start`, and the
/// walk then continues from that item's key.
pub(crate) fn digests_for_range(
    tree: &Tree,
    root: u64,
    csum_size: usize,
    sectorsize: u64,
    start: u64,
    len: u64,
) -> Result<SectorDigests> {
    let mut out = SectorDigests::new();
    if len == 0 || csum_size == 0 || sectorsize == 0 {
        return Ok(out);
    }
    let end = start.saturating_add(len);

    let target = DiskKey {
        objectid: EXTENT_CSUM_OBJECTID,
        key_type: EXTENT_CSUM_KEY,
        offset: start,
    };

    // Where to begin the walk: the item covering `start`, if the leaf
    // that would hold `start` has one before it.
    let mut from = target;
    let leaf = tree.descend(root, &target)?;
    if let Some(items) = leaf.body.items() {
        for item in items {
            if item.key.objectid != EXTENT_CSUM_OBJECTID
                || item.key.key_type != EXTENT_CSUM_KEY
                || item.key.offset > start
            {
                continue;
            }
            if item.key.offset >= from.offset || from.offset == start {
                from = item.key;
            }
        }
    }

    tree.for_each_from(root, &from, &mut |key: &DiskKey, data: &[u8]| {
        if key.objectid != EXTENT_CSUM_OBJECTID || key.key_type != EXTENT_CSUM_KEY {
            // Keys ascend, so anything else ends the run of csum items.
            return Ok(false);
        }
        if key.offset >= end {
            return Ok(false);
        }
        for (i, digest) in data.chunks_exact(csum_size).enumerate() {
            let sector = key.offset.saturating_add(i as u64 * sectorsize);
            if sector >= end {
                break;
            }
            if sector >= start {
                out.insert(sector, digest.to_vec());
            }
        }
        Ok(true)
    })?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btree::test_blocks::{geom, key, leaf, put_key, LEAF_A};

    /// A one-block tree: the leaf is the root.
    fn tree_over(block: Vec<u8>) -> (Vec<u8>, u64) {
        (block, LEAF_A)
    }

    /// One item covering four sectors, digests 4 bytes each.
    fn four_sector_item(first_logical: u64) -> (DiskKey, Vec<u8>) {
        let mut data = Vec::new();
        for i in 0..4u8 {
            data.extend_from_slice(&[0xA0 + i; 4]);
        }
        (
            key(EXTENT_CSUM_OBJECTID, EXTENT_CSUM_KEY, first_logical),
            data,
        )
    }

    /// A sector in the middle of a run is found, though no key names it.
    ///
    /// This is the lookup the tree's shape forces: one item covers many
    /// sectors, so the item for a sector is the last one at or before
    /// it. A search keyed on the sector's own address finds nothing for
    /// three sectors in every four here, and "nothing" reads as "no
    /// checksum to check".
    #[test]
    fn a_sector_inside_a_run_gets_the_digest_from_the_item_that_starts_before_it() {
        let sectorsize = 4096u64;
        let base = 1 << 20;
        let (k, data) = four_sector_item(base);
        let (block, at) = tree_over(leaf(LEAF_A, CSUM_TREE_OBJECTID, &[(k, data)]));
        let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
            assert_eq!(logical, at);
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);

        // The third sector of the run: 8 KiB into it.
        let third = base + 2 * sectorsize;
        let found = digests_for_range(&tree, at, 4, sectorsize, third, sectorsize).unwrap();
        assert_eq!(found.len(), 1, "the covering item was not found");
        assert_eq!(found[&third], vec![0xA2; 4]);
    }

    /// A range spanning the whole run gets one digest per sector.
    #[test]
    fn a_run_yields_one_digest_per_sector() {
        let sectorsize = 4096u64;
        let base = 1 << 20;
        let (k, data) = four_sector_item(base);
        let (block, at) = tree_over(leaf(LEAF_A, CSUM_TREE_OBJECTID, &[(k, data)]));
        let read = |_logical: u64, buf: &mut [u8]| -> Result<()> {
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);

        let found = digests_for_range(&tree, at, 4, sectorsize, base, 4 * sectorsize).unwrap();
        assert_eq!(found.len(), 4);
        for i in 0..4u64 {
            assert_eq!(found[&(base + i * sectorsize)], vec![0xA0 + i as u8; 4]);
        }
    }

    /// Sectors the tree does not name come back absent rather than as
    /// an error or a zero digest.
    ///
    /// This is the case a `NODATASUM` file presents, and treating it as
    /// a failure would refuse files the kernel reads happily.
    #[test]
    fn a_sector_no_item_covers_is_absent_rather_than_zero() {
        let sectorsize = 4096u64;
        let base = 1 << 20;
        let (k, data) = four_sector_item(base);
        let (block, at) = tree_over(leaf(LEAF_A, CSUM_TREE_OBJECTID, &[(k, data)]));
        let read = |_logical: u64, buf: &mut [u8]| -> Result<()> {
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);

        // Past the end of the run.
        let past = base + 4 * sectorsize;
        let found = digests_for_range(&tree, at, 4, sectorsize, past, sectorsize).unwrap();
        assert!(
            found.is_empty(),
            "invented a digest for an uncovered sector"
        );
    }

    /// The bytes of an item are read as `csum_size` digests, not as one.
    ///
    /// With a 32-byte digest — `sha256`, which `mkfs.btrfs` will make —
    /// the same item body describes a quarter as many sectors, and
    /// reading it four bytes at a time would compare the wrong bytes
    /// against every sector but the first.
    #[test]
    fn the_digest_length_decides_how_many_sectors_an_item_covers() {
        let sectorsize = 4096u64;
        let base = 1 << 20;
        let mut data = Vec::new();
        data.extend_from_slice(&[0x11u8; 32]);
        data.extend_from_slice(&[0x22u8; 32]);
        let k = key(EXTENT_CSUM_OBJECTID, EXTENT_CSUM_KEY, base);
        let (block, at) = tree_over(leaf(LEAF_A, CSUM_TREE_OBJECTID, &[(k, data)]));
        let read = |_logical: u64, buf: &mut [u8]| -> Result<()> {
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);

        let found = digests_for_range(&tree, at, 32, sectorsize, base, 4 * sectorsize).unwrap();
        assert_eq!(found.len(), 2, "64 bytes of sha256 is two sectors");
        assert_eq!(found[&base], vec![0x11; 32]);
        assert_eq!(found[&(base + sectorsize)], vec![0x22; 32]);
    }

    /// A key that is not a csum item does not become one.
    #[test]
    fn items_of_another_type_are_not_read_as_digests() {
        let sectorsize = 4096u64;
        let base = 1 << 20;
        let k = key(EXTENT_CSUM_OBJECTID, EXTENT_CSUM_KEY - 1, base);
        let (block, at) = tree_over(leaf(LEAF_A, CSUM_TREE_OBJECTID, &[(k, vec![0xFF; 16])]));
        let read = |_logical: u64, buf: &mut [u8]| -> Result<()> {
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);

        let found = digests_for_range(&tree, at, 4, sectorsize, base, sectorsize).unwrap();
        assert!(found.is_empty(), "read a non-csum item as digests");
    }

    /// Putting a key into a block is the helper's job; this asserts the
    /// helper import is used so the module compiles as written.
    #[test]
    fn key_encoding_round_trips() {
        let mut b = vec![0u8; 32];
        let k = key(EXTENT_CSUM_OBJECTID, EXTENT_CSUM_KEY, 4096);
        put_key(&mut b, 0, &k);
        assert_eq!(b[0..8], EXTENT_CSUM_OBJECTID.to_le_bytes());
        assert_eq!(b[8], EXTENT_CSUM_KEY);
    }
}

/// A checksum run that crosses a leaf boundary is not missed.
///
/// The predecessor lookup in `digests_for_range` finds the item covering
/// `start` by examining the leaf the descent lands on, and the walk that
/// follows relies on `for_each_from` to continue rightward past that
/// leaf's last item into the next one. Every other test in this module
/// builds a single-leaf, single-block tree, so neither half of that had
/// ever been exercised: the predecessor lookup only ever had one leaf to
/// search, and the walk never had a leaf boundary to cross. Filed as
/// rust-fs-btrfs#114 alongside the `.ok()` defect, as a claimed non-bug
/// that was nonetheless untested.
#[cfg(test)]
mod cross_leaf {
    use super::*;
    use crate::btree::test_blocks::{geom, key, leaf, node, LEAF_A, LEAF_B, ROOT};
    use crate::btree::Tree;
    use std::collections::HashMap;

    /// One four-sector run per leaf, back to back: leaf A covers
    /// sectors `[base, base+4)`, leaf B covers `[base+4, base+8)`. A
    /// root node with two key pointers, one per leaf, sits above them.
    fn two_leaf_tree(base: u64, sectorsize: u64) -> (HashMap<u64, Vec<u8>>, u64) {
        let mut a_data = Vec::new();
        let mut b_data = Vec::new();
        for i in 0..4u8 {
            a_data.extend_from_slice(&[0xA0 + i; 4]);
            b_data.extend_from_slice(&[0xB0 + i; 4]);
        }
        let a_key = key(EXTENT_CSUM_OBJECTID, EXTENT_CSUM_KEY, base);
        let b_key = key(EXTENT_CSUM_OBJECTID, EXTENT_CSUM_KEY, base + 4 * sectorsize);
        let a = leaf(LEAF_A, CSUM_TREE_OBJECTID, &[(a_key, a_data)]);
        let b = leaf(LEAF_B, CSUM_TREE_OBJECTID, &[(b_key, b_data)]);
        let r = node(
            ROOT,
            CSUM_TREE_OBJECTID,
            1,
            &[(a_key, LEAF_A), (b_key, LEAF_B)],
        );
        (HashMap::from([(LEAF_A, a), (LEAF_B, b), (ROOT, r)]), ROOT)
    }

    fn reader(blocks: &HashMap<u64, Vec<u8>>) -> impl Fn(u64, &mut [u8]) -> Result<()> + '_ {
        move |logical: u64, buf: &mut [u8]| -> Result<()> {
            let block = blocks
                .get(&logical)
                .unwrap_or_else(|| panic!("no block at {logical:#x} in this fixture"));
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        }
    }

    /// A range starting inside leaf A's run and ending inside leaf B's
    /// gets every sector from both, in one call.
    #[test]
    fn a_range_starting_in_one_leaf_and_ending_in_the_next_gets_both_halves() {
        let sectorsize = 4096u64;
        let base = 1 << 20;
        let (blocks, root) = two_leaf_tree(base, sectorsize);
        let read = reader(&blocks);
        let tree = Tree::new(geom(), &read);

        // Starts two sectors into leaf A's run (not at its first key --
        // that is the predecessor-lookup case) and runs four sectors,
        // ending two sectors into leaf B's run.
        let start = base + 2 * sectorsize;
        let found = digests_for_range(&tree, root, 4, sectorsize, start, 4 * sectorsize).unwrap();

        assert_eq!(
            found.len(),
            4,
            "expected 2 sectors from the tail of leaf A's run and 2 from \
             the head of leaf B's, got {}: {:?}",
            found.len(),
            found.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            found[&(base + 2 * sectorsize)],
            vec![0xA2; 4],
            "leaf A, sector 2"
        );
        assert_eq!(
            found[&(base + 3 * sectorsize)],
            vec![0xA3; 4],
            "leaf A, sector 3"
        );
        assert_eq!(
            found[&(base + 4 * sectorsize)],
            vec![0xB0; 4],
            "leaf B, sector 0"
        );
        assert_eq!(
            found[&(base + 5 * sectorsize)],
            vec![0xB1; 4],
            "leaf B, sector 1"
        );
    }

    /// The whole two-leaf tree in one call: eight sectors, two runs.
    #[test]
    fn the_entire_two_leaf_tree_is_read_in_one_call() {
        let sectorsize = 4096u64;
        let base = 1 << 20;
        let (blocks, root) = two_leaf_tree(base, sectorsize);
        let read = reader(&blocks);
        let tree = Tree::new(geom(), &read);

        let found = digests_for_range(&tree, root, 4, sectorsize, base, 8 * sectorsize).unwrap();
        assert_eq!(
            found.len(),
            8,
            "expected all 4+4 sectors across both leaves"
        );
        for i in 0..4u64 {
            assert_eq!(found[&(base + i * sectorsize)], vec![0xA0 + i as u8; 4]);
            assert_eq!(
                found[&(base + (4 + i) * sectorsize)],
                vec![0xB0 + i as u8; 4]
            );
        }
    }
}
