//! Cross-validation of the B-tree reader against real Btrfs media.
//!
//! The unit tests in `src/btree.rs` parse blocks that module encoded
//! itself, using the same offsets and strides it decodes with. That
//! proves the arithmetic is self-consistent; it cannot prove
//! `struct btrfs_header`, `struct btrfs_item` and `struct btrfs_key_ptr`
//! are laid out the way this crate believes, because a misreading would
//! be baked into both the fixture and the parser and they would agree
//! with each other while agreeing with nothing `mkfs.btrfs` produces.
//!
//! These tests close that gap by reading trees the kernel's own tooling
//! built. The chain they exercise is the real mount path and every link
//! is load-bearing:
//!
//! 1. parse the superblock at [`SUPER_INFO_OFFSET`];
//! 2. bootstrap the chunk map from its embedded system chunk array;
//! 3. translate `chunk_root` and parse the block that lands there;
//! 4. **walk the whole chunk tree** and fold every chunk item into the
//!    map;
//! 5. translate `root` — which is *only* reachable once step 4 has
//!    worked, because the root tree lives in a METADATA chunk that the
//!    bootstrap array does not describe — and parse the block there.
//!
//! Step 5 is the point of the file. A header offset that is off by a
//! byte, an item stride that is wrong, or a key comparison in the wrong
//! field order all make step 4 produce a map that cannot reach the root
//! tree, and the test fails loudly rather than quietly returning
//! plausible nonsense.
//!
//! # What these fixtures do not cover
//!
//! Every tree on a freshly-made, near-empty filesystem fits in a single
//! leaf, so these tests exercise [`TreeBlock`] parsing, leaf item bounds
//! and the leaf half of the search on real media — but **not** the
//! internal-node path: `struct btrfs_key_ptr`'s layout and the descent
//! through a level-1 node are still covered only by hand-built unit
//! tests. Closing that needs a fixture with enough files to force a tree
//! above level 0. The walk test below reports the tallest tree it
//! actually saw, so the gap stays visible rather than being quietly
//! assumed away.
//!
//! Fixtures live in `test-disks/` as `btrfs-<name>.img`. They are
//! gitignored and built by `chore fixtures`, which makes them with the
//! real tooling inside the fs-linux-test-harness VM. A checkout without
//! them fails here naming that task: these tests read the images
//! directly, so a missing one is nothing to run against, not a reason to
//! pass quietly.

use fs_btrfs::btree::{Tree, TreeBlock, TreeGeometry, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::{key_type, objectid, Chunk, ChunkMap, DiskKey};
use fs_btrfs::superblock::{Superblock, SUPER_INFO_OFFSET};
use fs_btrfs::{Error, Result};
use fs_btrfs_test_support::{fixtures_matching, spans_several_devices, Image};
use std::path::{Path, PathBuf};

/// `BTRFS_ROOT_ITEM_KEY`. `chunk::key_type` names only the two types the
/// chunk bootstrap needs, so this one is spelled out here rather than
/// added to a module this test is not allowed to touch.
const ROOT_ITEM_KEY: u8 = 132;

/// Every fixture image this file can read, with the multi-device ones
/// set aside.
fn fixtures() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut apart = Vec::new();
    for img in fixtures_matching("btrfs-") {
        let name = img.file_stem().unwrap().to_string_lossy().into_owned();

        // Multi-device filesystems are not fixtures for this file.
        // Everything here reads a logical address straight out of a
        // flat image, which is only meaningful when every chunk lives
        // on the device being read. One disk of a pool holds chunks
        // belonging to the other, and the bytes at those offsets are
        // something else entirely.
        if spans_several_devices(&img) {
            apart.push(name);
            continue;
        }
        out.push((name, img));
    }
    if !apart.is_empty() {
        eprintln!(
            "  {} multi-device image(s) read elsewhere: {apart:?}",
            apart.len()
        );
    }
    assert!(
        !out.is_empty(),
        "every fixture in test-disks/ spans several devices, so there is no flat \
         image here to read. `chore fixtures` builds the single-device matrix."
    );
    out
}

/// Open an image and parse its primary superblock.
///
/// The image is read a window at a time (see [`Image`]) rather than
/// loaded whole: the populated fixtures are 2 GiB each, and four test
/// threads holding one apiece is more memory than the harness guest has.
fn open(img: &Path, label: &str) -> (Image, Superblock) {
    let image = Image::open(img);
    let head = image.read_at(SUPER_INFO_OFFSET, 4096);
    let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET)
        .unwrap_or_else(|e| panic!("{label}: failed to parse a real superblock: {e}"));
    (image, sb)
}

/// Translate a logical address through `map` and copy the bytes out of
/// the flat image. Every fixture is single-device, so the mapping's
/// `devid` always names the one device the image is.
fn read_from(image: &Image, map: &ChunkMap, logical: u64, buf: &mut [u8]) -> Result<()> {
    let m = map.map(logical)?;
    if m.len < buf.len() as u64 {
        return Err(Error::Io(format!(
            "logical {logical:#x} has only {} contiguous bytes, need {}",
            m.len,
            buf.len()
        )));
    }
    if !image.try_read_at(m.physical, buf) {
        return Err(Error::Io(format!(
            "physical {}..{} is past the {}-byte image",
            m.physical,
            m.physical + buf.len() as u64,
            image.len()
        )));
    }
    Ok(())
}

/// Read one tree block through `map` and verify it.
fn block_at(image: &Image, map: &ChunkMap, sb: &Superblock, logical: u64) -> Result<TreeBlock> {
    let mut buf = vec![0u8; sb.nodesize as usize];
    read_from(image, map, logical, &mut buf)?;
    TreeBlock::parse(buf, logical, &TreeGeometry::from_superblock(sb))
}

/// Assert the things every tree block must be able to say about itself.
fn check_header(block: &TreeBlock, logical: u64, owner: u64, sb: &Superblock, label: &str) {
    assert_eq!(
        block.header.bytenr, logical,
        "{label}: block read from {logical:#x} claims to live at {:#x}",
        block.header.bytenr
    );
    assert_eq!(
        block.header.owner, owner,
        "{label}: block at {logical:#x} is owned by tree {} not {owner}",
        block.header.owner
    );
    assert_eq!(
        block.header.fsid,
        sb.node_uuid(),
        "{label}: block at {logical:#x} carries a foreign fsid"
    );
    let nritems = block.header.nritems as usize;
    assert!(
        nritems > 0,
        "{label}: block at {logical:#x} is empty — a live tree root never is"
    );
    // Plausibility: the entries have to fit. `ITEM_SIZE` is the smaller
    // of the two strides, so this bound holds for nodes and leaves
    // alike. TreeBlock::parse enforces the exact bound already; this
    // states it where a reader can see it.
    assert!(
        HEADER_SIZE + nritems * ITEM_SIZE <= sb.nodesize as usize,
        "{label}: block at {logical:#x} claims {nritems} entries, which cannot fit in {} bytes",
        sb.nodesize
    );
    // Level and body kind are decoded from different bytes: the level
    // from the header's last byte, the body from the stride the entries
    // were read at. They have to agree.
    assert_eq!(
        block.is_leaf(),
        block.body.items().is_some(),
        "{label}: block at {logical:#x} says level {} but its body says otherwise",
        block.header.level
    );
}

/// Bootstrap the chunk map and then extend it with every chunk item in
/// the chunk tree — the step that makes the rest of the volume
/// reachable.
///
/// Returns the extended map and the chunk items the walk found.
fn full_map(image: &Image, sb: &Superblock, label: &str) -> (ChunkMap, Vec<(DiskKey, Vec<u8>)>) {
    let boot = ChunkMap::bootstrap(sb)
        .unwrap_or_else(|e| panic!("{label}: chunk bootstrap failed on real media: {e}"));

    let read = |logical: u64, buf: &mut [u8]| read_from(image, &boot, logical, buf);
    let tree = Tree::from_superblock(sb, &read);

    let mut found = Vec::new();
    tree.for_each(sb.chunk_root, &mut |key, data| {
        if key.key_type == key_type::CHUNK_ITEM {
            found.push((*key, data.to_vec()));
        }
        Ok(true)
    })
    .unwrap_or_else(|e| panic!("{label}: walking the chunk tree failed: {e}"));

    let mut map = boot.clone();
    for (key, data) in &found {
        assert_eq!(
            key.objectid,
            objectid::FIRST_CHUNK_TREE,
            "{label}: chunk item filed under objectid {} not {}",
            key.objectid,
            objectid::FIRST_CHUNK_TREE
        );
        let chunk = Chunk::parse(key.offset, data).unwrap_or_else(|e| {
            panic!(
                "{label}: chunk item at {:#x} is unreadable: {e}",
                key.offset
            )
        });
        chunk.validate_geometry(sb.sectorsize).unwrap_or_else(|e| {
            panic!(
                "{label}: chunk item at {:#x} has bad geometry: {e}",
                key.offset
            )
        });
        // The SYSTEM chunks are in both the bootstrap array and the
        // tree; inserting one twice would be reported as an overlap.
        if !map.covers(chunk.logical) {
            map.insert(chunk).unwrap_or_else(|e| {
                panic!("{label}: chunk item at {:#x} overlaps: {e}", key.offset)
            });
        }
    }
    (map, found)
}

#[test]
fn chunk_tree_root_block_verifies_on_real_media() {
    let fixtures = fixtures();
    for (label, img) in &fixtures {
        let (image, sb) = open(img, label);
        let map = ChunkMap::bootstrap(&sb)
            .unwrap_or_else(|e| panic!("{label}: chunk bootstrap failed: {e}"));

        // Parsing at all means the checksum verified and both identity
        // fields agreed — on a block written by mkfs.btrfs, with
        // whichever of the four hash algorithms this fixture uses.
        let block = block_at(&image, &map, &sb, sb.chunk_root)
            .unwrap_or_else(|e| panic!("{label}: chunk tree root did not verify: {e}"));

        check_header(&block, sb.chunk_root, objectid::CHUNK_TREE, &sb, label);
        // The superblock independently records the chunk tree's height
        // and the transaction that wrote its root. Both must agree with
        // what the block says about itself; a header offset that slipped
        // would break at least one of them.
        assert_eq!(
            block.header.level, sb.chunk_root_level,
            "{label}: chunk root block says level {} but the superblock says {}",
            block.header.level, sb.chunk_root_level
        );
        assert_eq!(
            block.header.generation, sb.chunk_root_generation,
            "{label}: chunk root block says generation {} but the superblock says {}",
            block.header.generation, sb.chunk_root_generation
        );
        eprintln!(
            "  {label}: chunk root {:#x} level {}, {} entries, csum {:?}",
            sb.chunk_root, block.header.level, block.header.nritems, sb.csum_type
        );
    }
}

#[test]
fn walking_the_chunk_tree_makes_the_root_tree_reachable() {
    let fixtures = fixtures();
    let mut tallest = 0u8;
    for (label, img) in &fixtures {
        let (image, sb) = open(img, label);

        // Before the walk, the root tree is out of reach: it lives in a
        // METADATA chunk, and the bootstrap array describes only SYSTEM
        // chunks. If this ever stops holding, the test below stops
        // proving anything.
        let boot = ChunkMap::bootstrap(&sb).unwrap();
        assert!(
            boot.map(sb.root).is_err(),
            "{label}: the bootstrap map already reaches the root tree — \
             walking the chunk tree is no longer what makes it reachable"
        );

        let (map, chunk_items) = full_map(&image, &sb, label);
        tallest = tallest.max(sb.chunk_root_level).max(sb.root_level);
        assert!(
            !chunk_items.is_empty(),
            "{label}: the chunk tree walk found no chunk items"
        );
        assert!(
            map.len() > boot.len(),
            "{label}: the walk added no chunks to the bootstrap map"
        );

        let block = block_at(&image, &map, &sb, sb.root)
            .unwrap_or_else(|e| panic!("{label}: root tree block did not verify: {e}"));
        check_header(&block, sb.root, objectid::ROOT_TREE, &sb, label);
        assert_eq!(
            block.header.level, sb.root_level,
            "{label}: root tree block says level {} but the superblock says {}",
            block.header.level, sb.root_level
        );
        eprintln!(
            "  {label}: {} chunk items -> {} chunks; root tree {:#x} level {}, {} entries",
            chunk_items.len(),
            map.len(),
            sb.root,
            block.header.level,
            block.header.nritems
        );
    }
    if tallest == 0 {
        // Not a failure — but say it out loud. Every tree in the matrix
        // fit in one leaf, so nothing here exercised an internal node,
        // and `struct btrfs_key_ptr` remains validated only against
        // fixtures this crate built itself.
        eprintln!(
            "NOTE: every tree in the fixture matrix is level 0 — the internal-node \
             descent path was not exercised against real media"
        );
    }
}

#[test]
fn searching_finds_the_same_items_the_walk_found() {
    let fixtures = fixtures();
    for (label, img) in &fixtures {
        let (image, sb) = open(img, label);
        let boot = ChunkMap::bootstrap(&sb).unwrap();
        let read = |logical: u64, buf: &mut [u8]| read_from(&image, &boot, logical, buf);
        let tree = Tree::from_superblock(&sb, &read);

        // Sequential iteration and keyed descent are two different code
        // paths over the same tree. Every item the walk saw must come
        // back byte-identical through a search, or the key comparison
        // and the descent rule disagree with the order mkfs.btrfs wrote.
        let mut walked = Vec::new();
        tree.for_each(sb.chunk_root, &mut |key, data| {
            walked.push((*key, data.to_vec()));
            Ok(true)
        })
        .unwrap_or_else(|e| panic!("{label}: chunk tree walk failed: {e}"));
        assert!(
            walked.len() >= 2,
            "{label}: the chunk tree held only {} items — too few to be a real one",
            walked.len()
        );

        for (key, data) in &walked {
            let found = tree
                .search(sb.chunk_root, key)
                .unwrap_or_else(|e| panic!("{label}: search for {key:?} failed: {e}"))
                .unwrap_or_else(|| {
                    panic!("{label}: search could not find {key:?}, which the walk did")
                });
            assert_eq!(
                &found.data, data,
                "{label}: search returned different bytes for {key:?}"
            );
        }

        // The walk must have produced keys in ascending order, which is
        // the invariant the search depends on.
        for pair in walked.windows(2) {
            let (a, b) = (&pair[0].0, &pair[1].0);
            assert!(
                (a.objectid, a.key_type, a.offset) < (b.objectid, b.key_type, b.offset),
                "{label}: chunk tree walk produced {b:?} after {a:?}"
            );
        }

        // Device items and chunk items share the chunk tree, filed under
        // different objectids and types. Both runs must be findable by
        // prefix, and the counts must add up to the whole tree.
        let chunks = tree
            .find_all(
                sb.chunk_root,
                objectid::FIRST_CHUNK_TREE,
                key_type::CHUNK_ITEM,
            )
            .unwrap_or_else(|e| panic!("{label}: find_all for chunk items failed: {e}"));
        let devs = tree
            .find_all(sb.chunk_root, objectid::DEV_ITEMS, key_type::DEV_ITEM)
            .unwrap_or_else(|e| panic!("{label}: find_all for device items failed: {e}"));
        assert!(
            !chunks.is_empty(),
            "{label}: no chunk items found by prefix search"
        );
        assert_eq!(
            devs.len() as u64,
            sb.num_devices,
            "{label}: found {} device items but the superblock says {} devices",
            devs.len(),
            sb.num_devices
        );
        assert_eq!(
            chunks.len() + devs.len(),
            walked.len(),
            "{label}: the chunk tree holds items that are neither chunk nor device items"
        );
        eprintln!(
            "  {label}: {} chunk items + {} device items, all round-trip through search",
            chunks.len(),
            devs.len()
        );
    }
}

#[test]
fn the_root_tree_names_the_top_level_file_tree() {
    let fixtures = fixtures();
    for (label, img) in &fixtures {
        let (image, sb) = open(img, label);
        let (map, _) = full_map(&image, &sb, label);
        let read = |logical: u64, buf: &mut [u8]| read_from(&image, &map, logical, buf);
        let tree = Tree::from_superblock(&sb, &read);

        // Every Btrfs volume has a root item for the top-level file tree
        // (objectid 5). Finding it is the first thing a mount does after
        // the chunk map is complete, so it is the natural end of this
        // chain: it exercises the descent through a tree the bootstrap
        // could not even address.
        let roots = tree
            .find_all(sb.root, objectid::FS_TREE, ROOT_ITEM_KEY)
            .unwrap_or_else(|e| panic!("{label}: searching the root tree failed: {e}"));
        assert!(
            !roots.is_empty(),
            "{label}: the root tree holds no ROOT_ITEM for the file tree (objectid {}, type \
             {ROOT_ITEM_KEY}) — either the search is wrong or that key type number is",
            objectid::FS_TREE
        );

        // Round-trip the key the prefix search returned through an exact
        // search, so the exact path is exercised without this test
        // having to guess what the key's offset is.
        let first = &roots[0];
        let exact = tree
            .search(sb.root, &first.key)
            .unwrap_or_else(|e| panic!("{label}: exact search failed: {e}"))
            .unwrap_or_else(|| panic!("{label}: exact search missed {:?}", first.key));
        assert_eq!(exact.data, first.data, "{label}: the two searches disagree");

        // A key that cannot exist must come back as absent rather than
        // as the neighbouring item.
        assert!(
            tree.search(
                sb.root,
                &DiskKey {
                    objectid: u64::MAX,
                    key_type: u8::MAX,
                    offset: u64::MAX,
                }
            )
            .unwrap()
            .is_none(),
            "{label}: a search past the end of the tree returned something"
        );

        eprintln!(
            "  {label}: root tree holds {} file-tree root item(s), {} bytes each",
            roots.len(),
            first.data.len()
        );
    }
}
