//! A relocated tree holds what the original held.
//!
//! `render_plan` moves blocks: same contents, new addresses, and every
//! pointer that named an old address updated to the new one. The way
//! that goes wrong is not a crash — it is a node still pointing at where
//! a block used to be, which reads perfectly and returns the version
//! before the change.
//!
//! So the check walks the relocated tree and compares what it holds with
//! what the original held, item for item. Nothing is written to the
//! disk: the rendered blocks are held in memory and read back from
//! there, which is the same tree the commit sequencer would produce.
//!
//! The fixtures are gitignored and built by `chore fixtures`. A missing
//! one fails here: a relocation checked against no tree at all reads
//! exactly like one that preserved every item.

use fs_btrfs::btree::{header_offsets as o, HEADER_SIZE};
use fs_btrfs::chunk::{objectid, DiskKey};
use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{fixture, le32, le64};
use fs_core::FileDevice;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A fixture, mounted. An image that will not open or will not mount is
/// a failure rather than a test that quietly does nothing.
fn mounted(name: &str) -> Filesystem {
    let path = fixture(name);
    let dev = Arc::new(
        FileDevice::open(&path)
            .unwrap_or_else(|error| panic!("opening {}: {error}", path.display())),
    );
    Filesystem::mount(dev).unwrap_or_else(|error| panic!("mounting {}: {error}", path.display()))
}

/// The fs tree's root, from the root tree that names it.
fn fs_tree_root(fs: &Filesystem) -> u64 {
    /// `BTRFS_ROOT_ITEM_KEY`, and `btrfs_root_item.bytenr` within it.
    const ROOT_ITEM_KEY: u8 = 132;
    const ROOT_ITEM_BYTENR: usize = 176;
    fs.root_tree_items()
        .expect("reading the root tree")
        .into_iter()
        .find_map(|(objid, ty, _, data)| {
            (objid == objectid::FS_TREE
                && ty == ROOT_ITEM_KEY
                && data.len() >= ROOT_ITEM_BYTENR + 8)
                .then(|| le64(&data, ROOT_ITEM_BYTENR))
        })
        .expect("the root tree holds a ROOT_ITEM naming the fs tree's root")
}

/// The items of a leaf, as (key, bytes).
fn items_of(b: &[u8]) -> Vec<(DiskKey, Vec<u8>)> {
    let n = le32(b, o::NRITEMS) as usize;
    (0..n)
        .filter_map(|i| {
            let at = HEADER_SIZE + i * 25;
            let start = HEADER_SIZE + le32(b, at + 17) as usize;
            let end = start + le32(b, at + 21) as usize;
            (end <= b.len()).then(|| {
                (
                    DiskKey {
                        objectid: le64(b, at),
                        key_type: b[at + 8],
                        offset: le64(b, at + 9),
                    },
                    b[start..end].to_vec(),
                )
            })
        })
        .collect()
}

/// Walk a tree from `root`, reading blocks through `get`, and return
/// every item in key order.
///
/// Used on both the original and the relocated tree, so a difference is
/// a difference in the trees rather than in how they were read.
fn contents(root: u64, get: &dyn Fn(u64) -> Option<Vec<u8>>) -> Vec<(DiskKey, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(at) = stack.pop() {
        let Some(b) = get(at) else { continue };
        if b[o::LEVEL] == 0 {
            out.extend(items_of(&b));
            continue;
        }
        let n = le32(&b, o::NRITEMS) as usize;
        for i in (0..n).rev() {
            let p = HEADER_SIZE + i * 33;
            if p + 25 <= b.len() {
                stack.push(le64(&b, p + 17));
            }
        }
    }
    out.sort_by_key(|(k, _)| (k.objectid, k.key_type, k.offset));
    out
}

/// A rendered plan produces a tree with the same contents.
///
/// Both geometries, rather than whichever image was found first: a
/// 16 KiB node holds its items differently from a 4 KiB one, and a
/// relocation that preserved only the shape it was tried on would have
/// passed.
#[test]
fn a_relocated_tree_holds_exactly_what_the_original_held() {
    for name in ["btrfs-deep16k.img", "btrfs-default.img"] {
        a_relocated_tree_holds_what_it_held(name);
    }
}

fn a_relocated_tree_holds_what_it_held(name: &str) {
    let fs = mounted(name);
    let sb_root = fs.superblock().root;
    let generation = fs.superblock().generation + 1;

    // Move the whole root tree: dirty its root, which drags in nothing
    // below it but does move the block the superblock names.
    let plan = fs.plan_transaction(&[sb_root]).expect("planning");
    let blocks = fs.render_plan(&plan, generation).expect("rendering");
    assert_eq!(
        blocks.len(),
        plan.rewrites.len(),
        "{name}: every rewrite in the plan should produce a block"
    );

    let rendered: BTreeMap<u64, Vec<u8>> = blocks
        .iter()
        .map(|b| (b.logical, b.bytes.clone()))
        .collect();

    // Reading the new tree: a rendered block if there is one, otherwise
    // the block still on the disk. That is exactly what the filesystem
    // would look like once these blocks are committed.
    let after = |at: u64| -> Option<Vec<u8>> {
        rendered
            .get(&at)
            .cloned()
            .or_else(|| fs.read_tree_block(at).ok().map(|b| b.bytes().to_vec()))
    };
    let before =
        |at: u64| -> Option<Vec<u8>> { fs.read_tree_block(at).ok().map(|b| b.bytes().to_vec()) };

    let new_root = fs
        .planned_root(&plan)
        .expect("the plan moves the root tree");
    assert_ne!(
        new_root, sb_root,
        "{name}: the root tree did not actually move"
    );

    let was = contents(sb_root, &before);
    let now = contents(new_root, &after);

    assert!(!was.is_empty(), "{name}: the original tree read as empty");
    assert_eq!(
        now.len(),
        was.len(),
        "{name}: the relocated root tree holds {} items and the original held {}",
        now.len(),
        was.len()
    );
    assert_eq!(
        now, was,
        "{name}: the relocated root tree holds different items from the original"
    );
    eprintln!(
        "{name}: {} items preserved across a relocation of the root tree",
        was.len()
    );
}

/// Every rendered block says it is where it was put.
///
/// A block records its own address, and one stamped with the address it
/// came from is rejected when read at the new one — which is the failure
/// that catches a copy-on-write writer that forgot the block moved.
#[test]
fn every_rendered_block_is_stamped_with_its_new_address() {
    let fs = mounted("btrfs-default.img");
    let generation = fs.superblock().generation + 1;
    let plan = fs
        .plan_transaction(&[fs.superblock().root])
        .expect("planning");
    let blocks = fs.render_plan(&plan, generation).expect("rendering");

    for b in &blocks {
        assert_eq!(
            le64(&b.bytes, o::BYTENR),
            b.logical,
            "a block placed at {} claims to belong at {}",
            b.logical,
            le64(&b.bytes, o::BYTENR)
        );
        assert_eq!(
            le64(&b.bytes, o::GENERATION),
            generation,
            "a block written by transaction {generation} carries generation {}",
            le64(&b.bytes, o::GENERATION)
        );
        assert!(
            fs.superblock()
                .csum_type
                .verify(&b.bytes[32..], &b.bytes[..32]),
            "the block placed at {} carries a checksum that does not cover it",
            b.logical
        );
    }
    eprintln!(
        "{} rendered blocks, each stamped and checksummed",
        blocks.len()
    );
}

/// A ROOT_ITEM naming a tree that moved is updated.
///
/// The root tree holds the address of every other tree's root. If one
/// moves and its ROOT_ITEM is not rewritten, the filesystem still mounts
/// and still reads — from the tree as it was before the change.
#[test]
fn a_root_item_for_a_tree_that_moved_names_the_new_address() {
    let fs = mounted("btrfs-deep16k.img");
    const ROOT_ITEM_BYTENR: usize = 176;
    let generation = fs.superblock().generation + 1;

    // The fs tree's root, which the plan will move.
    let fs_root = fs_tree_root(&fs);

    let plan = fs.plan_transaction(&[fs_root]).expect("planning");
    let moved_to = plan
        .rewrites
        .iter()
        .find(|r| r.old == fs_root)
        .map(|r| r.new)
        .expect("the fs tree root is in the plan");

    let blocks = fs.render_plan(&plan, generation).expect("rendering");

    // Find the ROOT_ITEM for the fs tree in the rendered root tree.
    let mut found = None;
    for b in &blocks {
        if le64(&b.bytes, o::OWNER) != objectid::ROOT_TREE || b.bytes[o::LEVEL] != 0 {
            continue;
        }
        for (key, data) in items_of(&b.bytes) {
            if key.objectid == objectid::FS_TREE
                && key.key_type == 132
                && data.len() >= ROOT_ITEM_BYTENR + 8
            {
                found = Some(le64(&data, ROOT_ITEM_BYTENR));
            }
        }
    }

    let named = found.expect("the rendered root tree should hold a ROOT_ITEM for the fs tree");
    assert_eq!(
        named, moved_to,
        "the fs tree moved from {fs_root} to {moved_to}, and its ROOT_ITEM still names \
         {named}. The filesystem would mount and read the tree as it was before."
    );
    eprintln!("the fs tree's ROOT_ITEM follows it from {fs_root} to {moved_to}");
}

/// A node points at where its child went, not where it was.
///
/// This is the characteristic copy-on-write bug: the parent keeps the
/// old address, the tree still reads, and it returns the version before
/// the change. The tests above never reached it — they all planned from
/// a tree's ROOT, which has no parent, so no node ever had a moved
/// child and a renderer that simply did not update them passed
/// everything.
#[test]
fn a_node_follows_a_child_that_moved() {
    let fs = mounted("btrfs-deep16k.img");
    let generation = fs.superblock().generation + 1;
    let fs_root = fs_tree_root(&fs);

    // Descend to a leaf, remembering the path, so there is a real
    // parent/child pair to move.
    let mut path = vec![fs_root];
    while let Ok(block) = fs.read_tree_block(*path.last().unwrap()) {
        let Some(first) = block.body.key_ptrs().and_then(|p| p.first().copied()) else {
            break;
        };
        path.push(first.blockptr);
    }
    // `deep16k` holds 60,000 files, so its fs tree has nodes above its
    // leaves. One that came back a single block would mean the fixture
    // is not the one this test needs, and this test would otherwise
    // pass by checking nothing.
    assert!(
        path.len() >= 2,
        "the fs tree in btrfs-deep16k.img is a single block, so there is no parent to \
         follow a moved child"
    );

    let leaf = *path.last().unwrap();
    let parent = path[path.len() - 2];

    let plan = fs.plan_transaction(&[leaf]).expect("planning");
    let moved: BTreeMap<u64, u64> = plan.rewrites.iter().map(|r| (r.old, r.new)).collect();
    let leaf_to = *moved.get(&leaf).expect("the leaf is in the plan");
    let parent_to = *moved.get(&parent).expect("its parent is in the plan");

    let blocks = fs.render_plan(&plan, generation).expect("rendering");
    let rendered: BTreeMap<u64, Vec<u8>> = blocks
        .iter()
        .map(|b| (b.logical, b.bytes.clone()))
        .collect();

    let parent_block = rendered
        .get(&parent_to)
        .expect("the rendered parent should be among the blocks");
    let n = le32(parent_block, o::NRITEMS) as usize;
    let children: Vec<u64> = (0..n)
        .map(|i| le64(parent_block, HEADER_SIZE + i * 33 + 17))
        .collect();

    assert!(
        children.contains(&leaf_to),
        "the leaf moved from {leaf} to {leaf_to} and its parent, now at {parent_to}, \
         points at {children:?}. A node naming the old address is a tree that reads the \
         version before the change."
    );
    assert!(
        !children.contains(&leaf),
        "the parent at {parent_to} still points at the leaf's OLD address {leaf}"
    );

    // A key pointer carries the generation of the child it names, and
    // the kernel checks it on read — a mismatch is "parent transid
    // verify failed", which refuses the block. This driver's reader
    // does NOT check it, so nothing else here would notice.
    let slot = children
        .iter()
        .position(|c| *c == leaf_to)
        .expect("the moved leaf is among the children");
    let child_gen = le64(parent_block, HEADER_SIZE + slot * 33 + 25);
    assert_eq!(
        child_gen, generation,
        "the parent names the leaf at {leaf_to} with generation {child_gen}, but it was \
         written by transaction {generation}. The kernel refuses that block on read with \
         a parent transid mismatch."
    );

    // And the subtree still reads the same.
    let after = |at: u64| -> Option<Vec<u8>> {
        rendered
            .get(&at)
            .cloned()
            .or_else(|| fs.read_tree_block(at).ok().map(|b| b.bytes().to_vec()))
    };
    let before =
        |at: u64| -> Option<Vec<u8>> { fs.read_tree_block(at).ok().map(|b| b.bytes().to_vec()) };
    let was = contents(parent, &before);
    let now = contents(parent_to, &after);
    assert_eq!(
        now, was,
        "the relocated subtree under {parent_to} holds different items from the original \
         under {parent}"
    );

    eprintln!(
        "a leaf {} levels down moved to {leaf_to}; its parent followed, and {} items \
         still read the same",
        path.len() - 1,
        was.len()
    );
}
