//! `fs.btrfs` against btrfs-progs, every tool call inside the harness VM.
//!
//! - `get` agrees with `btrfs inspect-internal dump-super` field by field
//!   on volumes mkfs.btrfs made: the CLI's own volume, and one per
//!   checksum algorithm.
//! - A volume whose fs tree root fails its checksum in every copy, and one
//!   whose superblock magic is gone from every copy, are refused by
//!   `fs.btrfs` -- a structured error, and not one byte on stdout -- AND by
//!   `btrfs check --readonly`. Where the damage goes is found by
//!   btrfs-progs (`dump-tree`, `btrfs-map-logical`), not by this crate, so
//!   the test cannot share a misreading with the code it checks.

mod cli_support;

use cli_support::*;
use fs_btrfs_test_support::{dump_super, dump_tree, fixture, oracle};
use std::path::Path;

/// The value of `name` in a dump-super: the rest of the first line whose
/// first word it is.
fn super_field(dump: &str, name: &str) -> String {
    dump.lines()
        .find_map(|l| {
            let mut words = l.split_whitespace();
            (words.next() == Some(name)).then(|| words.collect::<Vec<_>>().join(" "))
        })
        .unwrap_or_else(|| panic!("dump-super has no {name}:\n{dump}"))
}

fn get(image: &Path, key: &str) -> String {
    let out = ok(tool("fs.btrfs").arg(image).args(["get", key, "--text"]));
    stdout(&out).trim_end().to_string()
}

#[test]
fn get_agrees_with_dump_super() {
    let mut images = vec![fixture("cli/btrfs-cli.img")];
    for csum in ["crc32c", "xxhash", "sha256", "blake2"] {
        images.push(fixture(&format!("btrfs-csum-{csum}.img")));
    }
    for image in &images {
        let dump = dump_super(image);
        let at = image.display();
        let label = super_field(&dump, "label");
        assert_eq!(get(image, "label"), label, "{at}: label");
        assert_eq!(get(image, "btrfs.fsid"), super_field(&dump, "fsid"), "{at}");
        let sector = super_field(&dump, "sectorsize");
        assert_eq!(get(image, "block_size"), sector, "{at}: sectorsize");
        assert_eq!(get(image, "btrfs.sector_size"), sector, "{at}");
        assert_eq!(
            get(image, "btrfs.node_size"),
            super_field(&dump, "nodesize"),
            "{at}"
        );
        // `csum_type		0 (crc32c)`: the name in brackets.
        let csum = super_field(&dump, "csum_type");
        let name = csum
            .split_once('(')
            .and_then(|(_, rest)| rest.strip_suffix(')'))
            .unwrap_or_else(|| panic!("{at}: csum_type {csum:?}"));
        assert_eq!(get(image, "btrfs.csum_type"), name, "{at}: csum_type");
        assert_eq!(
            get(image, "total_bytes"),
            super_field(&dump, "total_bytes"),
            "{at}"
        );
        assert_eq!(
            get(image, "btrfs.bytes_used"),
            super_field(&dump, "bytes_used"),
            "{at}"
        );
        assert_eq!(
            get(image, "btrfs.device_count"),
            super_field(&dump, "num_devices"),
            "{at}"
        );
        assert_eq!(
            get(image, "btrfs.generation"),
            super_field(&dump, "generation"),
            "{at}"
        );
    }
    assert_eq!(get(&images[0], "label"), "CLITEST");
}

/// A scratch copy of the CLI volume.
fn scratch_copy(tag: &str) -> std::path::PathBuf {
    let copy = scratch_dir(tag).join("btrfs-cli.img");
    std::fs::copy(fixture("cli/btrfs-cli.img"), &copy).expect("copy the fixture");
    copy
}

/// Where btrfs-progs says every copy of the block at `logical` lives.
fn physical_copies(image: &Path, logical: u64) -> Vec<u64> {
    let out = oracle("btrfs-map-logical")
        .args(["-l", &logical.to_string()])
        .arg(image)
        .output();
    assert!(
        out.status.success(),
        "btrfs-map-logical -l {logical}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let copies: Vec<u64> = text
        .lines()
        .filter(|l| l.starts_with("mirror"))
        .filter_map(|l| {
            let words: Vec<&str> = l.split_whitespace().collect();
            let at = words.iter().position(|w| *w == "physical")?;
            words.get(at + 1)?.parse().ok()
        })
        .collect();
    assert!(
        !copies.is_empty(),
        "btrfs-map-logical named no copy:\n{text}"
    );
    copies
}

/// Every command that reads the trees fails with a structured error and
/// prints nothing -- not one byte of a block that failed its checksum.
fn assert_refused_without_output(image: &Path) {
    for args in [
        vec!["ls", "/"],
        vec!["read", "/hello.txt"],
        vec!["ls", "/vol"],
    ] {
        let message = refused(tool("fs.btrfs").arg(image).args(&args), 1);
        assert!(!message.is_empty(), "{args:?}");
    }
}

fn assert_btrfs_check_refuses(image: &Path, what: &str) {
    let out = oracle("btrfs")
        .args(["check", "--readonly"])
        .arg(image)
        .output();
    assert_ne!(
        out.status.code(),
        Some(0),
        "btrfs check --readonly passed {what}, so the damage is not what this test says it is:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn a_tree_block_failing_its_checksum_in_every_copy_is_refused_by_both() {
    let image = scratch_copy("broken-tree-block");
    // The fs tree's root, from the root tree as btrfs-progs prints it:
    // `item N key (FS_TREE ROOT_ITEM 0) ...` then `... bytenr B level L`.
    let roots = dump_tree(&image, "root");
    let item = roots
        .find("(FS_TREE ROOT_ITEM 0)")
        .unwrap_or_else(|| panic!("no FS_TREE ROOT_ITEM in:\n{roots}"));
    let logical: u64 = roots[item..]
        .split_whitespace()
        .skip_while(|w| *w != "bytenr")
        .nth(1)
        .and_then(|w| w.parse().ok())
        .unwrap_or_else(|| panic!("no bytenr after FS_TREE ROOT_ITEM:\n{}", &roots[item..]));
    // One byte past the header, in every copy: the block no longer
    // matches the checksum in its header.
    use std::os::unix::fs::FileExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&image)
        .unwrap();
    for physical in physical_copies(&image, logical) {
        let mut byte = [0u8; 1];
        file.read_exact_at(&mut byte, physical + 0x100).unwrap();
        byte[0] ^= 0xff;
        file.write_all_at(&byte, physical + 0x100).unwrap();
    }
    file.sync_all().unwrap();
    drop(file);

    assert_refused_without_output(&image);
    let message = refused(tool("fs.btrfs").arg(&image).args(["read", "/hello.txt"]), 1);
    assert!(message.contains("checksum"), "{message}");
    // The superblock is intact, so `get` still answers.
    assert_eq!(get(&image, "label"), "CLITEST");
    assert_btrfs_check_refuses(&image, "a volume whose fs tree root fails its checksum");
}

#[test]
fn a_superblock_without_its_magic_in_every_copy_is_refused_by_both() {
    let image = scratch_copy("no-magic");
    use std::os::unix::fs::FileExt;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&image)
        .unwrap();
    // The primary at 64 KiB and the mirror at 64 MiB; a 512 MiB volume has
    // no copy at 256 GiB. The magic is 8 bytes at 0x40 in each.
    for copy in [64u64 << 10, 64 << 20] {
        file.write_all_at(b"XXXXXXXX", copy + 0x40).unwrap();
    }
    file.sync_all().unwrap();
    drop(file);

    assert_refused_without_output(&image);
    refused(tool("fs.btrfs").arg(&image).arg("get"), 1);
    assert_btrfs_check_refuses(&image, "a volume with no superblock magic");
}
