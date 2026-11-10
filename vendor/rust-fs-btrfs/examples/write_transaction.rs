//! Perform one transaction on a copy of a filesystem.
//!
//! Plans a relocation, renders it, and commits — the whole write path
//! end to end. The result is meant to be handed to `btrfs check` and to
//! the kernel's own mount, which is the only judgement that counts.
//!
//!   cargo run --example write_transaction -- source.img out.img

use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::Commit;
use fs_core::{BlockDevice, FileDevice};
use std::sync::Arc;

fn main() {
    let src = std::env::args().nth(1).expect("source image");
    let out = std::env::args().nth(2).expect("output image");
    std::fs::copy(&src, &out).expect("copying the image");

    let dev = Arc::new(FileDevice::open_rw(&out).expect("opening writable"));
    let fs = Filesystem::mount_rw(dev.clone() as Arc<dyn BlockDevice>).expect("mounting");

    let generation = fs.superblock().generation + 1;
    let root = fs.superblock().root;
    println!(
        "generation {} -> {generation}, root tree at {root}",
        fs.superblock().generation
    );

    // Move the root tree, and everything moving that implies.
    let plan = fs
        .plan_transaction_closed(&[root], 8)
        .expect("planning a closed transaction");
    println!(
        "plan: {} blocks, trees {:?}",
        plan.rewrites.len(),
        plan.trees()
    );
    for r in &plan.rewrites {
        println!(
            "   {} -> {}  (tree {}, level {})",
            r.old, r.new, r.owner, r.level
        );
    }

    let blocks = fs.render_plan(&plan, generation).expect("rendering");
    let new_root = fs
        .planned_root(&plan)
        .expect("the plan moves the root tree");

    fs.commit(
        &blocks,
        &Commit {
            generation,
            root: new_root,
            root_level: None,
            bytes_used: None,
            chunk_root: None,
            chunk_root_generation: None,
            // Kept `true` deliberately, and NOT for the reason this
            // comment used to give.
            //
            // The old reason — "this transaction does not maintain the
            // free-space tree" — stopped being true when
            // `apply_free_space` landed: `render_plan` rewrites the
            // free-space tree's extents alongside the extent tree's,
            // and refuses outright (rather than silently skipping) a
            // block group recorded as a bitmap.
            //
            // The reason now is that this example does not *verify*
            // what it wrote. Clearing the validity bit tells the kernel
            // to rebuild the cache on the next read-write mount, which
            // is the format's own way of saying "believe the extent
            // tree, not this". For an example whose point is the
            // transaction shape, that is the honest setting.
            //
            // A caller that runs the oracle suite — which checks the
            // result with `btrfs check` inside a VM — can set this to
            // `false` and keep the cache. Setting it to `false` here,
            // in an example nothing checks, would be asserting a
            // property this file does not test.
            invalidate_free_space_tree: true,
        },
    )
    .expect("committing");

    println!("committed: root tree now at {new_root}");
}
