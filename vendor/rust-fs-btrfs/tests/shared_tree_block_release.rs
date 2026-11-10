//! A transaction refuses to release a tree block a snapshot still shares
//! (#178).
//!
//! `apply_records` released each block a plan rewrites by deleting its
//! `METADATA_ITEM`, without reading its reference count. After a snapshot
//! of a tree whose root is a node, the blocks under the root are referenced
//! by both trees. Rewriting one deleted its record outright, so the
//! snapshot's tree pointed at a block the allocator then considered free.
//! Dropping one reference, and adding explicit ones for a shared block's
//! children when it's copied, is what the kernel does. Until this driver
//! does the same, the plan must be refused.
//!
//! The image is `snapshot/btrfs-nodatacow-snapshot.img` from `chore
//! fixtures`, a read-only snapshot the kernel took of the top-level
//! subvolume. btrfs-progs — in the harness VM, where the tools are —
//! names a tree block the extent tree records with two references, and
//! the planner and renderer are asked to move it.

use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{dump_tree, fixture};
use fs_core::FileDevice;
use std::path::Path;
use std::sync::Arc;

/// A tree block the extent tree records with more than one reference.
fn shared_tree_block(image: &Path) -> u64 {
    let dump = dump_tree(image, "extent");
    let lines: Vec<&str> = dump.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let Some(key) = line.split("key (").nth(1) else {
            continue;
        };
        let mut words = key.split_whitespace();
        let (Some(bytenr), Some("METADATA_ITEM")) = (words.next(), words.next()) else {
            continue;
        };
        let refs = lines
            .get(i + 1)
            .and_then(|l| l.split_whitespace().skip_while(|w| *w != "refs").nth(1))
            .and_then(|r| r.parse::<u64>().ok());
        if refs.is_some_and(|r| r > 1) {
            return bytenr.parse().unwrap();
        }
    }
    panic!("the fixture holds no tree block with more than one reference, so it no longer holds the case");
}

#[test]
fn a_plan_that_releases_a_shared_tree_block_is_refused() {
    let image = fixture("snapshot/btrfs-nodatacow-snapshot.img");
    let shared = shared_tree_block(&image);
    let fs = Filesystem::mount(Arc::new(FileDevice::open(&image).unwrap())).expect("mount");
    let generation = fs.superblock().generation + 1;
    let rendered = fs.plan_transaction_closed(&[shared], 8).and_then(|plan| {
        assert!(
            plan.released().contains(&shared),
            "the plan does not move the shared block, so it tests nothing"
        );
        fs.render_plan(&plan, generation)
    });
    assert!(
        rendered.is_err(),
        "a transaction moving the tree block at {shared}, which a snapshot shares, \
         was rendered: its extent record would be deleted while the snapshot still \
         points at it"
    );
}
