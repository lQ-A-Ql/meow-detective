//! Multi-level tree descent, validated against real media.
//!
//! `tests/btree_oracle.rs` walks the chunk tree and the root tree. On a
//! freshly made filesystem both are a single leaf, so everything the
//! B-tree reader does *above* level 0 — internal node parsing, the
//! `KeyPtr` layout, and the descent loop itself — went unexercised by
//! real data. Those constants rested only on hand-built blocks, which is
//! exactly the position this project treats as unvalidated.
//!
//! This file closes that gap. It reaches the **fs tree**, which is where
//! file and directory metadata lives and which grows past a single leaf
//! as soon as a filesystem holds a few thousand files. The
//! `btrfs-deep4k` and `btrfs-deep16k` fixtures are built with 20,000 and
//! 60,000 files respectively and carry level-2 fs trees.
//!
//! Getting there is itself the test. The fs tree root is not named by
//! the superblock: you must map the chunk tree, walk it to complete the
//! address map, read the root tree, find the `ROOT_ITEM` for the fs
//! tree, and pull the root address out of it. Every one of those steps
//! has to be right before the first internal node is even read.
//!
//! The fixtures are gitignored and built by `chore fixtures`, in the
//! fs-linux-test-harness VM. A missing one fails here rather than
//! excusing the walk: this file used to return early when it found
//! none, and a level-2 descent that never happened reads exactly like
//! one that did.

use fs_btrfs::btree::{Tree, TreeGeometry};
use fs_btrfs::chunk::{ChunkMap, DiskKey};
use fs_btrfs::superblock::{Superblock, SUPER_INFO_OFFSET};
use fs_btrfs_test_support::{fixtures_matching, spans_several_devices, Image};

/// One window of the image, as the tree reader wants it: an error rather
/// than a panic, because a chunk map that points past the end of the
/// device is exactly what these oracles are here to catch.
fn read_window(image: &Image, physical: u64, buf: &mut [u8]) -> fs_btrfs::Result<()> {
    if image.try_read_at(physical, buf) {
        return Ok(());
    }
    Err(fs_btrfs::Error::Io(format!(
        "physical {physical}..{} is past the {}-byte image",
        physical + buf.len() as u64,
        image.len()
    )))
}
use std::path::PathBuf;

/// `BTRFS_FS_TREE_OBJECTID` — the subvolume holding the default
/// filesystem namespace.
const FS_TREE_OBJECTID: u64 = 5;

/// `BTRFS_ROOT_ITEM_KEY`.
const ROOT_ITEM_KEY: u8 = 132;

/// Byte offset of `bytenr` within `struct btrfs_root_item`.
///
/// The item opens with an embedded `btrfs_inode_item`, which is 160
/// bytes, followed by `generation` and `root_dirid` before the root
/// address. Confirmed by the assertions below: a wrong value here yields
/// an address that fails the tree block's own identity check rather than
/// silently producing plausible garbage.
const ROOT_ITEM_BYTENR_OFFSET: usize = 176;

/// Byte offset of `level` within `struct btrfs_root_item`.
const ROOT_ITEM_LEVEL_OFFSET: usize = 238;

/// Every fixture whose blocks can be read straight out of the image,
/// with the label a failure names it by.
fn fixtures() -> Vec<(String, PathBuf)> {
    let images: Vec<PathBuf> = fixtures_matching("btrfs-")
        .into_iter()
        // One member of a multi-device filesystem holds chunks that
        // live on the other disk; reading them out of this image
        // returns bytes that fail a checksum against a block they were
        // never meant to be.
        .filter(|p| !spans_several_devices(p))
        .collect();
    assert!(
        !images.is_empty(),
        "every fixture belongs to a multi-device filesystem, so there is no image here \
         whose tree blocks can be read flat"
    );
    images
        .into_iter()
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), p))
        .collect()
}

/// The populated fixtures, which are the ones built deep enough to
/// carry a level-2 fs tree.
fn deep_fixtures() -> Vec<(String, PathBuf)> {
    fixtures_matching("btrfs-deep")
        .into_iter()
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), p))
        .collect()
}

/// Complete the address map by walking the chunk tree, then locate the
/// fs tree root and its level.
fn fs_tree_root(image: &Image, sb: &Superblock, label: &str) -> Option<(ChunkMap, u64, u8)> {
    let boot =
        ChunkMap::bootstrap(sb).unwrap_or_else(|e| panic!("{label}: chunk bootstrap failed: {e}"));

    // Read through the bootstrap map first, folding every chunk item the
    // chunk tree holds into a complete map.
    let read = |logical: u64, buf: &mut [u8]| -> fs_btrfs::Result<()> {
        let m = boot.map(logical)?;
        read_window(image, m.physical, buf)
    };
    let mut map = boot.clone();
    let tree = Tree::from_superblock(sb, &read);
    tree.for_each(sb.chunk_root, &mut |key, data| {
        if let Ok(chunk) = fs_btrfs::chunk::Chunk::parse(key.offset, data) {
            let _ = map.insert(chunk);
        }
        Ok(true)
    })
    .unwrap_or_else(|e| panic!("{label}: walking the chunk tree failed: {e}"));

    // Now the root tree is reachable. Find the fs tree's ROOT_ITEM.
    let read_full = |logical: u64, buf: &mut [u8]| -> fs_btrfs::Result<()> {
        let m = map.map(logical)?;
        read_window(image, m.physical, buf)
    };
    let root_tree = Tree::from_superblock(sb, &read_full);

    let mut found = None;
    root_tree
        .for_each(sb.root, &mut |key: &DiskKey, data: &[u8]| {
            if key.objectid == FS_TREE_OBJECTID && key.key_type == ROOT_ITEM_KEY {
                assert!(
                    data.len() > ROOT_ITEM_LEVEL_OFFSET,
                    "{label}: root item is only {} bytes, too short to hold its level",
                    data.len()
                );
                let bytenr = u64::from_le_bytes(
                    data[ROOT_ITEM_BYTENR_OFFSET..ROOT_ITEM_BYTENR_OFFSET + 8]
                        .try_into()
                        .unwrap(),
                );
                found = Some((bytenr, data[ROOT_ITEM_LEVEL_OFFSET]));
            }
            Ok(true)
        })
        .unwrap_or_else(|e| panic!("{label}: walking the root tree failed: {e}"));

    let (bytenr, level) = found?;
    Some((map, bytenr, level))
}

/// Every fixture's fs tree must be reachable and internally consistent.
#[test]
fn fs_tree_is_reachable_and_walkable() {
    let mut deepest = 0u8;
    for (label, img) in &fixtures() {
        let image = Image::open(img);
        let head = image.read_at(SUPER_INFO_OFFSET, 4096);
        let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("parse superblock");
        let Some((map, root, level)) = fs_tree_root(&image, &sb, label) else {
            panic!("{label}: no ROOT_ITEM for the fs tree — the root tree walk is wrong");
        };

        let read = |logical: u64, buf: &mut [u8]| -> fs_btrfs::Result<()> {
            let m = map.map(logical)?;
            read_window(&image, m.physical, buf)
        };
        let tree = Tree::new(TreeGeometry::from_superblock(&sb), &read);

        // The root block must verify: checksum, its own address, and the
        // filesystem it claims to belong to.
        let block = tree
            .read_block(root)
            .unwrap_or_else(|e| panic!("{label}: fs tree root at {root:#x} failed to parse: {e}"));
        assert_eq!(
            block.header.level, level,
            "{label}: the root item says level {level}, the block header says {}",
            block.header.level
        );
        assert_eq!(
            block.header.owner, FS_TREE_OBJECTID,
            "{label}: fs tree root is owned by tree {} not {FS_TREE_OBJECTID}",
            block.header.owner
        );

        // Walk every item. On a multi-level tree this descends through
        // internal nodes, which is the path no other test reaches.
        let mut items = 0usize;
        tree.for_each(root, &mut |_key, _data| {
            items += 1;
            Ok(true)
        })
        .unwrap_or_else(|e| panic!("{label}: walking the fs tree failed: {e}"));

        assert!(
            items > 0,
            "{label}: the fs tree walk produced no items at all"
        );
        eprintln!("  {label}: fs tree level {level}, {items} items");
        deepest = deepest.max(level);
    }

    assert!(
        deepest > 0,
        "every fixture has a level-0 fs tree, so internal node parsing and \
         multi-level descent are still unvalidated against real media. The \
         deep4k/deep16k fixtures exist to prevent exactly this — check they \
         were generated."
    );
    eprintln!("deepest fs tree walked: level {deepest}");
}

/// Items returned by a full walk must come back identically through a
/// keyed search. On a multi-level tree this proves the descent picks the
/// same leaf the sequential walk reached.
#[test]
fn keyed_search_agrees_with_the_walk_on_a_multi_level_tree() {
    for (label, img) in &deep_fixtures() {
        let image = Image::open(img);
        let head = image.read_at(SUPER_INFO_OFFSET, 4096);
        let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("parse superblock");
        let (map, root, level) = fs_tree_root(&image, &sb, label).expect("fs tree root");
        assert!(
            level > 0,
            "{label} is meant to be a multi-level fixture but its fs tree is level {level}"
        );

        let read = |logical: u64, buf: &mut [u8]| -> fs_btrfs::Result<()> {
            let m = map.map(logical)?;
            read_window(&image, m.physical, buf)
        };
        let tree = Tree::new(TreeGeometry::from_superblock(&sb), &read);

        // Sample across the whole key range rather than the first few,
        // so the descent has to choose between different subtrees.
        let mut sampled: Vec<(DiskKey, Vec<u8>)> = Vec::new();
        let mut seen = 0usize;
        tree.for_each(root, &mut |key, data| {
            if seen.is_multiple_of(500) {
                sampled.push((*key, data.to_vec()));
            }
            seen += 1;
            Ok(true)
        })
        .expect("walk");

        assert!(
            sampled.len() > 4,
            "{label}: only {} samples from {seen} items — too few to exercise descent",
            sampled.len()
        );

        for (key, expected) in &sampled {
            let block = tree
                .descend(root, key)
                .unwrap_or_else(|e| panic!("{label}: descend to {key:?} failed: {e}"));
            let items = block.body.items().unwrap_or(&[]);
            let hit = items
                .iter()
                .find(|it| fs_btrfs::btree::compare_keys(&it.key, key) == std::cmp::Ordering::Equal)
                .unwrap_or_else(|| {
                    panic!("{label}: descend landed on a leaf not containing {key:?}")
                });
            let data = block.item_data(hit).expect("item data in range");
            assert_eq!(
                data, expected,
                "{label}: descend and walk disagree on the data for {key:?}"
            );
        }
        eprintln!(
            "  {label}: level {level}, {seen} items, {} keys re-found by descent",
            sampled.len()
        );
    }
}
