//! A commit fills its backup slot with the roots it left (#78).
//!
//! `mkfs.btrfs --rootdir` makes a volume; the crate commits two
//! transactions on it, each moving the root tree (the shape
//! `examples/write_transaction.rs` performs). After each, btrfs-progs reads
//! the result back: `btrfs check` must pass, and the backup slot
//! `dump-super -f` shows for the new generation must name the tree root,
//! chunk root, size and usage the superblock holds and the extent,
//! filesystem, device and checksum roots `dump-tree -t root` lists. Before
//! this, the slot still described a commit the kernel made. The tools run
//! in the harness VM, which is the one place they live, so they are always
//! there and nothing here skips.

use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::Commit;
use fs_btrfs_test_support::{assert_btrfs_check_clean, dump_super, dump_tree, oracle, temp_path};
use fs_core::{BlockDevice, FileDevice};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// `(bytenr, generation, level)` from a `dump-super -f` backup line.
fn triple(line: &str) -> (u64, u64, u8) {
    let nums: Vec<u64> = line
        .split_whitespace()
        .filter_map(|w| w.parse().ok())
        .collect();
    (nums[0], nums[1], nums[2] as u8)
}

/// The backup slot for `generation`, as btrfs-progs decodes it.
fn backup_slot(image: &Path, generation: u64) -> BTreeMap<String, String> {
    let dump = dump_super(image);
    let header = format!("backup {}:", (generation - 1) % 4);
    let mut out = BTreeMap::new();
    for line in dump
        .lines()
        .skip_while(|l| l.trim() != header)
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
    {
        if let Some((k, v)) = line.trim().split_once(':') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

/// `(bytenr, generation, level)` of each global tree's `ROOT_ITEM`.
fn root_items(image: &Path) -> BTreeMap<String, (u64, u64, u8)> {
    let dump = dump_tree(image, "root");
    let mut out = BTreeMap::new();
    let lines: Vec<&str> = dump.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix("item ") else {
            continue;
        };
        let Some(name) = rest
            .split("key (")
            .nth(1)
            .and_then(|k| k.split_whitespace().next())
        else {
            continue;
        };
        if !rest.contains("ROOT_ITEM 0)") {
            continue;
        }
        let word = |key: &str, from: &str| -> u64 {
            let mut it = from.split_whitespace();
            while let Some(w) = it.next() {
                if w == key {
                    return it.next().unwrap().parse().unwrap();
                }
            }
            panic!("{key} not in {from}")
        };
        let gen = word("generation", lines[i + 1]);
        let bytenr = word("bytenr", lines[i + 1]);
        let level = word("level", lines[i + 4]) as u8;
        out.insert(name.to_string(), (bytenr, gen, level));
    }
    out
}

#[test]
fn a_commit_fills_its_backup_slot() {
    // Scratch inside this repository: the guest that runs the tools sees
    // this repository and nothing else of the host, so an image anywhere
    // else is a path they cannot open.
    let root = std::path::PathBuf::from(temp_path!("backup-ring"));
    std::fs::create_dir_all(root.join("tree")).unwrap();
    for i in 0..50 {
        std::fs::write(root.join(format!("tree/f{i}")), format!("file {i}")).unwrap();
    }
    let image = root.join("fs.img");
    std::fs::File::create(&image)
        .and_then(|f| f.set_len(256 * 1024 * 1024))
        .unwrap();
    let out = oracle("mkfs.btrfs")
        .args(["-q", "-f", "-s", "4096", "-n", "16384", "--rootdir"])
        .arg(root.join("tree"))
        .arg(&image)
        .output();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    for _ in 0..2 {
        let generation = {
            let dev = Arc::new(FileDevice::open_rw(&image).unwrap());
            let fs = Filesystem::mount_rw(dev as Arc<dyn BlockDevice>).expect("mount rw");
            let generation = fs.superblock().generation + 1;
            let plan = fs
                .plan_transaction_closed(&[fs.superblock().root], 8)
                .expect("plan");
            let blocks = fs.render_plan(&plan, generation).expect("render");
            let new_root = fs
                .planned_root(&plan)
                .expect("the plan moves the root tree");
            fs.commit(
                &blocks,
                &Commit {
                    generation,
                    root: new_root,
                    invalidate_free_space_tree: true,
                    ..Default::default()
                },
            )
            .expect("commit");
            generation
        };

        assert_btrfs_check_clean(&image, &format!("generation {generation}"));
        let slot = backup_slot(&image, generation);
        let sb = dump_super(&image);
        let field = |name: &str| -> String {
            sb.lines()
                .find(|l| l.split_whitespace().next() == Some(name))
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or_else(|| panic!("{name} in dump-super"))
                .to_string()
        };
        let trees = root_items(&image);

        assert_eq!(
            triple(&slot["backup_tree_root"]),
            (
                field("root").parse().unwrap(),
                generation,
                field("root_level").parse().unwrap()
            ),
            "generation {generation}: tree root"
        );
        assert_eq!(
            triple(&slot["backup_chunk_root"]),
            (
                field("chunk_root").parse().unwrap(),
                field("chunk_root_generation").parse().unwrap(),
                field("chunk_root_level").parse().unwrap()
            ),
            "generation {generation}: chunk root"
        );
        for (label, tree) in [
            ("backup_extent_root", "EXTENT_TREE"),
            ("backup_fs_root", "FS_TREE"),
            ("backup_dev_root", "DEV_TREE"),
            ("csum_root", "CSUM_TREE"),
        ] {
            assert_eq!(
                triple(&slot[label]),
                trees[tree],
                "generation {generation}: {label}"
            );
        }
        assert_eq!(slot["backup_total_bytes"], field("total_bytes"));
        assert_eq!(slot["backup_bytes_used"], field("bytes_used"));
        assert_eq!(slot["backup_num_devices"], field("num_devices"));
    }
    let _ = std::fs::remove_dir_all(&root);
}
