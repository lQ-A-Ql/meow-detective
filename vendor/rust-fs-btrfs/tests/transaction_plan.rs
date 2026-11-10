//! What a planned transaction contains, checked against a real one.
//!
//! `Filesystem::plan_transaction` works out which blocks a change makes
//! the filesystem rewrite. `docs/cow-transaction.md` measured what the
//! kernel rewrote for one `touch` on the same filesystem, so the plan
//! has something to be compared against that was not written to fit it.
//!
//! The comparison is a containment, not an equality, and the reason is
//! the planner's own documented boundary: it computes the spine — the
//! dirty blocks, their ancestors, and the root tree leaf naming the
//! tree whose root moved — and does not model the extent, free-space and
//! dev tree rewrites that recording those allocations causes. So the
//! trees it names must be among the trees the kernel touched, and must
//! include the ones the spine implies.
//!
//! # Both geometries, every run
//!
//! The COW pairs exist twice: on a default `mkfs.btrfs`, and on a volume
//! made with `--csum sha256 -d dup -m dup`. Which one these tests
//! planned against used to depend on `BTRFS_COW_SUFFIX`, set by the
//! workflow that ran the suite — so a plan was only ever held to the DUP
//! geometry where somebody had remembered to ask for it, and a job that
//! stopped setting it would have taken that coverage away silently. Both
//! are required here, both are planned against, and a failure names the
//! one it came from.
//!
//! The fixtures are gitignored and built by `chore fixtures`.

use fs_btrfs::chunk::objectid;
use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{fixture, Image};
use fs_core::FileDevice;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

/// The two geometries the COW fixtures are built on: how a failure names
/// one, and the suffix its images carry.
const GEOMETRIES: [(&str, &str); 2] = [("default geometry", ""), ("sha256+dup", "-sha256-dup")];

/// A fixture, mounted. An image that will not open or will not mount is
/// a failure rather than a test that quietly does nothing.
fn mounted(name: &str) -> Filesystem {
    let path = fixture(name);
    mount(&path)
}

fn mount(path: &Path) -> Filesystem {
    let dev = Arc::new(
        FileDevice::open(path)
            .unwrap_or_else(|error| panic!("opening {}: {error}", path.display())),
    );
    Filesystem::mount(dev).unwrap_or_else(|error| panic!("mounting {}: {error}", path.display()))
}

/// The before image of one geometry's COW pair.
fn cow_before(suffix: &str) -> Filesystem {
    mounted(&format!("btrfs-cow-before{suffix}.img"))
}

/// The fs tree's root, which is the block a change to a file reaches.
fn fs_tree_root(fs: &Filesystem) -> u64 {
    /// `BTRFS_ROOT_ITEM_KEY`, and `btrfs_root_item.bytenr` within it.
    const ROOT_ITEM_KEY: u8 = 132;
    const BYTENR: usize = 176;
    fs.root_tree_items()
        .expect("reading the root tree")
        .into_iter()
        .find(|(objid, ty, _, data)| {
            *objid == objectid::FS_TREE && *ty == ROOT_ITEM_KEY && data.len() >= BYTENR + 8
        })
        .map(|(_, _, _, data)| u64::from_le_bytes(data[BYTENR..BYTENR + 8].try_into().unwrap()))
        .expect("the root tree holds a ROOT_ITEM naming the fs tree's root")
}

/// A plan for changing the fs tree covers the spine above it.
#[test]
fn a_change_to_the_fs_tree_rewrites_it_and_the_root_tree() {
    for (label, suffix) in GEOMETRIES {
        let fs = cow_before(suffix);
        let fs_root = fs_tree_root(&fs);

        let plan = fs.plan_transaction(&[fs_root]).expect("planning");
        let trees = plan.trees();

        assert!(
            trees.contains(&objectid::FS_TREE),
            "[{label}] the fs tree's own root was made dirty and the plan does not \
             rewrite it: {trees:?}"
        );
        assert!(
            trees.contains(&objectid::ROOT_TREE),
            "[{label}] the fs tree's root moved, so the ROOT_ITEM naming it changed and \
             the root tree leaf holding it must be rewritten. The plan stops short: \
             {trees:?}"
        );

        // The block that changed is in the plan, and it is going somewhere
        // else — a copy-on-write rewrite that lands on the same address is
        // an overwrite.
        let it = plan
            .rewrites
            .iter()
            .find(|r| r.old == fs_root)
            .expect("the dirty block itself must be in the plan");
        assert_ne!(
            it.new, it.old,
            "[{label}] the plan rewrites {fs_root} onto itself, which is not \
             copy-on-write"
        );

        eprintln!(
            "[{label}] {} blocks, trees {:?}",
            plan.rewrites.len(),
            trees.iter().collect::<Vec<_>>()
        );
    }
}

/// Every tree the plan names is one the kernel also rewrote.
///
/// The kernel's transaction for the same change is the upper bound: a
/// plan that touches a tree the kernel did not is rewriting something
/// for no reason, which costs a block and, once the extent tree is being
/// edited, records an allocation nothing needed.
#[test]
fn the_plan_touches_no_tree_the_kernel_left_alone() {
    for (label, suffix) in GEOMETRIES {
        let before = cow_before(suffix);
        let path = fixture(&format!("btrfs-cow-after{suffix}.img"));
        let after = mount(&path);
        let fs_root = fs_tree_root(&before);

        // Which trees the kernel rewrote: any block in the after image
        // newer than the before image belongs to one.
        let old_gen = before.superblock().generation;
        // A block at a time rather than the whole image: see `Image`.
        let image = Image::open(&path);
        let sb = after.superblock();
        let n = sb.nodesize as usize;
        let mut kernel: BTreeSet<u64> = BTreeSet::new();
        let mut buf = vec![0u8; n];
        let mut at = 0u64;
        while image.try_read_at(at, &mut buf) {
            let b = &buf[..];
            at += n as u64;
            if b[0x20..0x30] != sb.fsid[..] {
                continue;
            }
            if !sb.csum_type.verify(&b[32..], &b[..32]) {
                continue;
            }
            let gen = u64::from_le_bytes(b[0x50..0x58].try_into().unwrap());
            if gen > old_gen {
                kernel.insert(u64::from_le_bytes(b[0x58..0x60].try_into().unwrap()));
            }
        }

        // The after image is one `touch` and one `sync` past the before
        // image, so it HAS to hold newer blocks. An empty set is a
        // fixture that did not capture what it says it captured, and
        // treating it as "nothing to compare" is how a comparison
        // against nothing reads as a pass.
        assert!(
            !kernel.is_empty(),
            "[{label}] the after image holds no block newer than generation {old_gen}, so \
             the kernel's own transaction is not in the fixture at all"
        );

        let plan = before.plan_transaction(&[fs_root]).expect("planning");
        for tree in plan.trees() {
            assert!(
                kernel.contains(&tree),
                "[{label}] the plan rewrites tree {tree}, which the kernel did not touch \
                 for the same change. It rewrote {kernel:?}."
            );
        }
        eprintln!(
            "[{label}] plan touches {:?}, kernel touched {:?}",
            plan.trees().iter().collect::<Vec<_>>(),
            kernel.iter().collect::<Vec<_>>()
        );
    }
}

/// Nothing is placed where something already is.
#[test]
fn every_new_address_is_free_distinct_and_aligned() {
    for (label, suffix) in GEOMETRIES {
        let fs = cow_before(suffix);
        let fs_root = fs_tree_root(&fs);
        let nodesize = fs.superblock().nodesize as u64;

        let plan = fs.plan_transaction(&[fs_root]).expect("planning");
        assert!(
            !plan.rewrites.is_empty(),
            "[{label}] an empty plan tests nothing"
        );

        // Distinct: two blocks sharing an address is one block.
        let news: BTreeSet<u64> = plan.allocated().into_iter().collect();
        assert_eq!(
            news.len(),
            plan.rewrites.len(),
            "[{label}] the plan places {} blocks at {} distinct addresses",
            plan.rewrites.len(),
            news.len()
        );

        // Free, according to the extent tree — the addresses the plan takes
        // must not be ones already holding something.
        let groups: Vec<_> = fs
            .block_groups()
            .expect("block groups")
            .into_iter()
            .filter(|g| g.holds_metadata())
            .collect();
        let free = fs.free_extents_by_group(&groups).expect("free space");
        for at in &news {
            assert_eq!(at % nodesize, 0, "[{label}] {at} is not tree-block aligned");
            let covered = free
                .iter()
                .flatten()
                .any(|r| *at >= r.start && at + nodesize <= r.end());
            assert!(
                covered,
                "[{label}] the plan puts a block at {at}, which the extent tree says is \
                 allocated. Writing there overwrites live data and the filesystem still \
                 mounts."
            );
        }

        // One release per allocation, so usage does not move.
        assert_eq!(
            plan.usage_delta(nodesize),
            0,
            "[{label}] a plan that only rewrites should not change how much is used"
        );
        eprintln!(
            "[{label}] {} placements, all free, distinct and aligned",
            news.len()
        );
    }
}

/// A block nothing points at cannot be planned around.
#[test]
fn planning_an_unreachable_block_is_refused() {
    for (label, suffix) in GEOMETRIES {
        let fs = cow_before(suffix);
        // An address inside the filesystem but holding no tree block.
        let nowhere = fs.superblock().root + fs.superblock().nodesize as u64 * 1_000;
        let err = fs
            .plan_transaction(&[nowhere])
            .expect_err("a block that is part of no tree has nothing above it to rewrite");
        assert!(
            err.to_string().contains("not reachable"),
            "[{label}] the refusal should say why: {err}"
        );
    }
}

/// A change to a leaf rewrites every node above it.
///
/// The other tests plan from a tree's root, which has no ancestors — so
/// they never exercise the walk upwards at all, and a planner that
/// simply did not do it passed every one of them. That is what a
/// surviving mutation looks like, and it is why this test uses a
/// filesystem deep enough to have something above a leaf.
#[test]
fn a_change_to_a_leaf_rewrites_the_nodes_above_it() {
    // The deep geometries are the ones built with enough files to push
    // a tree past a single block, and both are used: a 4 KiB node and a
    // 16 KiB one reach a different number of levels, so a planner that
    // stopped one short would show up in one and not the other.
    for name in ["btrfs-deep16k.img", "btrfs-deep4k.img"] {
        let fs = mounted(name);
        let fs_root = fs_tree_root(&fs);

        // Descend to a leaf, remembering the path. Anything with a node
        // above it will do.
        let mut path = vec![fs_root];
        while let Ok(block) = fs.read_tree_block(*path.last().unwrap()) {
            let Some(first) = block.body.key_ptrs().and_then(|p| p.first().copied()) else {
                break;
            };
            path.push(first.blockptr);
        }

        // Both deep fixtures hold tens of thousands of files, so their fs
        // trees have nodes above their leaves. A single block would mean
        // the fixture is not the one this test needs, and the walk
        // upwards would go untested while the test still passed.
        assert!(
            path.len() >= 2,
            "{name}: the fs tree is a single block, so there is nothing above a leaf to \
             check"
        );

        let leaf = *path.last().unwrap();
        let plan = fs.plan_transaction(&[leaf]).expect("planning");
        let rewritten: BTreeSet<u64> = plan.rewrites.iter().map(|r| r.old).collect();

        // Every block on the path from the leaf up to the tree's root.
        for at in &path {
            assert!(
                rewritten.contains(at),
                "{name}: the leaf at {leaf} was made dirty and {at} is above it on the \
                 path to the root, but the plan leaves it alone. A node still pointing at \
                 the leaf's OLD address is a tree that reads the version before the change."
            );
        }

        assert!(
            plan.trees().contains(&objectid::ROOT_TREE),
            "{name}: the fs tree's root moved and the root tree was not rewritten"
        );
        eprintln!(
            "{name}: a leaf {} levels down: {} blocks rewritten, whole path included",
            path.len() - 1,
            plan.rewrites.len()
        );
    }
}

/// The recursion settles.
///
/// Moving a block means recording the move, and those records live in
/// extent tree leaves that are then themselves blocks to move. That is
/// the recursion `docs/cow-transaction.md` measured, where a commit
/// changing nothing still rewrote four blocks. It terminates because
/// each rewrite is an allocation AND a release, so the extent tree ends
/// up recording its own new blocks rather than growing.
///
/// Whether it terminates *for this implementation* is not something the
/// measurement settles — that is what this asks.
#[test]
fn closing_the_plan_over_its_own_bookkeeping_settles() {
    for (label, suffix) in GEOMETRIES {
        let fs = cow_before(suffix);
        let fs_root = fs_tree_root(&fs);

        let open = fs.plan_transaction(&[fs_root]).expect("the spine alone");
        let closed = fs
            .plan_transaction_closed(&[fs_root], 8)
            .expect("the plan should settle: recording a move must not keep making more work");

        assert!(
            closed.rewrites.len() >= open.rewrites.len(),
            "[{label}] closing the plan lost blocks: {} became {}",
            open.rewrites.len(),
            closed.rewrites.len()
        );

        // The extent tree must be in it. That is the whole point: the spine
        // alone does not record its own moves.
        assert!(
            closed.trees().contains(&objectid::EXTENT_TREE),
            "[{label}] a closed plan must rewrite the extent tree, because that is where \
             the record of every move it makes lives. It touches {:?}",
            closed.trees().iter().collect::<Vec<_>>()
        );

        // Still nothing the kernel left alone.
        eprintln!(
            "[{label}] spine {} blocks {:?} -> closed {} blocks {:?}",
            open.rewrites.len(),
            open.trees().iter().collect::<Vec<_>>(),
            closed.rewrites.len(),
            closed.trees().iter().collect::<Vec<_>>()
        );
    }
}

/// A closed plan still places everything somewhere free and distinct.
#[test]
fn a_closed_plan_places_every_block_somewhere_free() {
    for (label, suffix) in GEOMETRIES {
        let fs = cow_before(suffix);
        let fs_root = fs_tree_root(&fs);
        let nodesize = fs.superblock().nodesize as u64;
        let plan = fs
            .plan_transaction_closed(&[fs_root], 8)
            .expect("closing a plan over its own bookkeeping");

        let news: BTreeSet<u64> = plan.allocated().into_iter().collect();
        assert_eq!(
            news.len(),
            plan.rewrites.len(),
            "[{label}] a closed plan places {} blocks at {} distinct addresses",
            plan.rewrites.len(),
            news.len()
        );

        // Nothing is placed on a block the same plan is moving away from —
        // legal in principle, since that address is freed, but only once the
        // transaction commits, and writing there first destroys the source.
        let olds: BTreeSet<u64> = plan.released().into_iter().collect();
        for at in &news {
            assert!(
                !olds.contains(at),
                "[{label}] the plan puts a new block at {at}, which is where a block it is \
                 still reading from lives. That address is only free after the commit."
            );
        }

        let groups: Vec<_> = fs
            .block_groups()
            .expect("block groups")
            .into_iter()
            .filter(|g| g.holds_metadata())
            .collect();
        let free = fs.free_extents_by_group(&groups).expect("free space");
        for at in &news {
            assert_eq!(at % nodesize, 0, "[{label}] {at} is not tree-block aligned");
            assert!(
                free.iter()
                    .flatten()
                    .any(|r| *at >= r.start && at + nodesize <= r.end()),
                "[{label}] the plan puts a block at {at}, which the extent tree says is \
                 allocated"
            );
        }
        eprintln!(
            "[{label}] {} blocks, all free, distinct, and none on top of a source",
            news.len()
        );
    }
}
