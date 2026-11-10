//! A transaction on a block group whose free-space records span two leaves
//! either leaves a free-space tree `btrfs check` accepts, or is refused by
//! name (#177).
//!
//! The free-space tree files each block group's `FREE_SPACE_INFO` item and
//! then its `FREE_SPACE_EXTENT`s in key order, and a leaf can end anywhere
//! among them. The rewrite read a group's run from the leaf holding its INFO
//! item to that leaf's end, and wrote every run of the group back into that
//! leaf. The next leaf kept its own copies of the group's tail, so the tree
//! then recorded that free space twice, out of key order across the leaf
//! boundary: `btrfs check` reports "free space extent ... overlaps with
//! previous". When the group did not fit in the first leaf the render was
//! refused instead, by a message about leaf sizes that did not say why.
//!
//! THE IMAGE IS MADE BY THE KERNEL, in the harness guest, because the shape
//! is the kernel's decision: which leaf boundary falls inside which group.
//! A 4 KiB-node volume with mixed block groups, so the data fragmentation
//! lands in the same groups metadata is allocated from, and a free-space
//! leaf holds ~160 records. `btrfs balance` first relocates mkfs's small
//! first group, which metadata churn would otherwise turn into bitmaps
//! (its threshold is ~11 runs) and which the allocator would then pick
//! for every new block — a bitmap is refused for its own reason before
//! this one is reached. Then 1 MiB files fill most of the volume and every
//! k-th is deleted: each deletion frees exactly one run, and with k = 5 a
//! 320 MiB group holds ~100 runs, well under the kernel's threshold for
//! converting it to bitmaps (~1.4 runs per MiB) while the groups together
//! span several leaves.
//!
//! WHICH GROUPS STRADDLE IS READ FROM btrfs-progs, not from this crate: a
//! group counts when `dump-tree` shows its records in more than one leaf,
//! its INFO item says extents rather than bitmaps, and a leaf of the fs,
//! csum or extent tree lives inside it for the transaction to move. The
//! layout is the kernel's and moves between kernel versions, so each
//! spacing is tried in turn; one that yields no such group is reported
//! with what it did yield, not passed.

use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::Commit;
use fs_btrfs_test_support::{
    assert_btrfs_check_clean, dump_tree, guest_kernel_write_ok, oracle, temp_path,
};
use fs_core::{BlockDevice, FileDevice};
use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every file the recipe writes is this long, and so is each run a deletion frees.
const FILE_MIB: u64 = 1;
/// How many files fill the volume: about two thirds of it.
const FILES: u64 = 2000;
/// The volume. Its block groups are 320 MiB, a tenth of it.
const VOLUME: u64 = 3 << 30;
/// The spacings tried, in order: every k-th file is deleted.
const SPACINGS: [u64; 3] = [5, 4, 6];
/// The text the refusal carries.
const REFUSAL: &str = "rewriting a group across a leaf boundary is not implemented";

/// A kernel-made volume with every `k`-th file deleted.
fn fragmented(dir: &Path, k: u64) -> PathBuf {
    let image = dir.join(format!("every-{k}th.img"));
    std::fs::File::create(&image)
        .and_then(|f| f.set_len(VOLUME))
        .unwrap();
    let made = oracle("mkfs.btrfs")
        .args(["-q", "-f", "-M", "-n", "4096", "-s", "4096"])
        .arg(&image)
        .output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    // Zeros: the content is never read, and the copy back to the host is
    // sparse, so the gigabytes written cost nothing to move.
    let script = format!(
        "btrfs balance start --full-balance \"$MNT\" >/dev/null\n\
         for f in $(seq 0 {last}); do head -c {bytes} /dev/zero > \"$MNT/f$f\"; done\n\
         sync\n\
         for f in $(seq 0 {k} {last}); do rm \"$MNT/f$f\"; done\n\
         sync",
        last = FILES - 1,
        bytes = FILE_MIB << 20,
    );
    guest_kernel_write_ok(
        &image.to_string_lossy(),
        &format!("every {k}th file deleted"),
        &script,
    );
    image
}

/// One block group, as btrfs-progs describes it.
#[derive(Debug)]
struct Group {
    start: u64,
    length: u64,
    metadata: bool,
    bitmaps: bool,
    leaves: BTreeSet<usize>,
}

/// The numbers inside `key (A TYPE B)` on an `item` line.
fn item_key(line: &str) -> Option<(u64, &str, u64)> {
    let line = line.trim_start();
    if !line.starts_with("item ") {
        return None;
    }
    let key = line.split("key (").nth(1)?;
    let mut words = key.split_whitespace();
    let objectid = words.next()?.parse().ok()?;
    let kind = words.next()?;
    let offset = words.next()?.trim_end_matches(')').parse().ok()?;
    Some((objectid, kind, offset))
}

/// Every group the free-space tree describes, with the leaves its records sit in.
fn free_space_groups(image: &Path) -> Vec<Group> {
    let mut metadata = BTreeSet::new();
    let extent = dump_tree(image, "extent");
    let lines: Vec<&str> = extent.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if let Some((start, "BLOCK_GROUP_ITEM", _)) = item_key(line) {
            if lines.get(i + 1).is_some_and(|l| l.contains("METADATA")) {
                metadata.insert(start);
            }
        }
    }

    let fst = dump_tree(image, "10");
    let lines: Vec<&str> = fst.lines().collect();
    let mut groups: Vec<Group> = Vec::new();
    let mut leaf = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("leaf ") {
            leaf += 1;
            continue;
        }
        let Some((objectid, kind, offset)) = item_key(line) else {
            continue;
        };
        match kind {
            "FREE_SPACE_INFO" => {
                let flags = lines
                    .get(i + 1)
                    .and_then(|l| l.split_whitespace().last())
                    .and_then(|f| f.parse::<u32>().ok())
                    .expect("a FREE_SPACE_INFO item is followed by its flags");
                groups.push(Group {
                    start: objectid,
                    length: offset,
                    metadata: metadata.contains(&objectid),
                    bitmaps: flags & 1 != 0,
                    leaves: BTreeSet::from([leaf]),
                });
            }
            "FREE_SPACE_EXTENT" | "FREE_SPACE_BITMAP" => {
                if let Some(g) = groups
                    .iter_mut()
                    .rev()
                    .find(|g| objectid >= g.start && objectid < g.start + g.length)
                {
                    g.leaves.insert(leaf);
                }
            }
            _ => {}
        }
    }
    assert!(
        !groups.is_empty(),
        "btrfs-progs shows no FREE_SPACE_INFO item: the volume has no free-space tree"
    );
    groups
}

/// The leaves of the fs, csum and extent trees.
fn tree_leaves(image: &Path) -> Vec<u64> {
    ["5", "7", "2"]
        .iter()
        .flat_map(|tree| {
            dump_tree(image, tree)
                .lines()
                .filter_map(|l| l.strip_prefix("leaf "))
                .filter_map(|l| l.split_whitespace().next()?.parse::<u64>().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A copy of `from` at `to` that keeps its holes, so a 3 GiB volume costs
/// what it holds.
fn sparse_copy(from: &Path, to: &Path) {
    let mut src = std::fs::File::open(from).unwrap();
    let mut dst = std::fs::File::create(to).unwrap();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = src.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        if buf[..n].iter().all(|b| *b == 0) {
            dst.seek(SeekFrom::Current(n as i64)).unwrap();
        } else {
            dst.write_all(&buf[..n]).unwrap();
        }
    }
    dst.set_len(std::fs::metadata(from).unwrap().len()).unwrap();
}

/// Move `dirty`, keeping the free-space tree valid, and commit.
fn transaction(image: &Path, dirty: u64) -> fs_btrfs::Result<()> {
    let dev = Arc::new(FileDevice::open_rw(image).unwrap());
    let fs = Filesystem::mount_rw(dev as Arc<dyn BlockDevice>).expect("mount rw");
    let generation = fs.superblock().generation + 1;
    let plan = fs.plan_transaction_closed(&[dirty], 64)?;
    let blocks = fs.render_plan(&plan, generation)?;
    let root = fs
        .planned_root(&plan)
        .expect("the plan moves the root tree");
    fs.commit(
        &blocks,
        &Commit {
            generation,
            root,
            invalidate_free_space_tree: false,
            ..Default::default()
        },
    )
}

#[test]
fn a_transaction_on_a_straddling_group_keeps_the_free_space_tree_valid_or_is_refused() {
    let dir = PathBuf::from(temp_path!("fst-straddle"));
    std::fs::create_dir_all(&dir).unwrap();

    let mut tried = Vec::new();
    for k in SPACINGS {
        let image = fragmented(&dir, k);
        let groups = free_space_groups(&image);
        let leaves = tree_leaves(&image);
        let candidates: Vec<(&Group, u64)> = groups
            .iter()
            .filter(|g| g.metadata && !g.bitmaps && g.leaves.len() > 1)
            .filter_map(|g| {
                let inside = leaves
                    .iter()
                    .copied()
                    .find(|b| *b >= g.start && *b < g.start + g.length)?;
                Some((g, inside))
            })
            .collect();
        if candidates.is_empty() {
            tried.push(format!("every {k}th file deleted: {groups:?}"));
            let _ = std::fs::remove_file(&image);
            continue;
        }

        for (group, dirty) in &candidates {
            let copy = dir.join(format!("every-{k}th-{}.img", group.start));
            sparse_copy(&image, &copy);
            let label = format!(
                "every {k}th file deleted; group {}+{}, records in {} leaves, moving the leaf at {dirty}",
                group.start,
                group.length,
                group.leaves.len()
            );
            let outcome = match transaction(&copy, *dirty) {
                Ok(()) => {
                    assert_btrfs_check_clean(&copy, &label);
                    "committed, and btrfs check accepts it".to_string()
                }
                Err(e) => {
                    assert!(
                        e.to_string().contains(REFUSAL),
                        "[{label}] refused, but not for the straddle: {e}"
                    );
                    "refused by name".to_string()
                }
            };
            println!("[{label}] {outcome}");
            let _ = std::fs::remove_file(&copy);
        }
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    panic!(
        "no spacing produced a mixed group, recorded as extents, whose free-space records \
         straddle a leaf and which holds a tree leaf to move:\n{}",
        tried.join("\n")
    );
}

/// The other side: a group whose records sit in one leaf is rewritten, not
/// refused, and `btrfs check` accepts the free-space tree that leaves.
#[test]
fn a_transaction_on_a_group_in_one_leaf_keeps_the_free_space_tree_valid() {
    let dir = PathBuf::from(temp_path!("fst-one-leaf"));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("fs.img");
    std::fs::File::create(&image)
        .and_then(|f| f.set_len(256 << 20))
        .unwrap();
    let made = oracle("mkfs.btrfs")
        .args(["-q", "-f", "-M", "-n", "4096", "-s", "4096"])
        .arg(&image)
        .output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    let groups = free_space_groups(&image);
    assert!(
        groups.iter().all(|g| g.leaves.len() == 1 && !g.bitmaps),
        "a fresh volume should keep every group's records, as extents, in one leaf: {groups:?}"
    );

    let root = {
        let fs = Filesystem::mount(Arc::new(FileDevice::open(&image).unwrap())).unwrap();
        fs.tree_root_public(5).expect("the fs tree")
    };
    transaction(&image, root).expect("a group in one leaf is not refused");
    assert_btrfs_check_clean(&image, "a transaction on a group in one leaf");
    let _ = std::fs::remove_dir_all(&dir);
}
