//! A lookup fetches the name's `DIR_ITEM` by key rather than listing the
//! directory (#64).
//!
//! `mkfs.btrfs --rootdir` builds a directory of 20,000 names, two of which
//! share a name hash, so their records are packed into one `DIR_ITEM`. Every
//! name `read_dir` lists must look up to the inode it lists; names that are
//! not there, including one that shares a present name's hash, must not.
//!
//! And the difference must show. Resolving every one of the 20,000 names by
//! listing the directory each time is 20,000 x 20,000 entries materialised
//! -- minutes. By key it is 20,000 map lookups. The bound below is two orders
//! of magnitude above the keyed cost and two below the listing one.
//!
//! `mkfs.btrfs` runs in the fs-linux-test-harness VM, like every oracle
//! tool, so one version of btrfs-progs builds this directory on every
//! machine — and the tree it is built from lives inside this repository,
//! which is the only part of the host the guest can see. Nothing skips:
//! a guest that cannot run the tool fails the test naming the task that
//! provisions it.

use fs_btrfs::{dir, Filesystem};
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::FileDevice;
use std::sync::Arc;
use std::time::{Duration, Instant};

const NAMES: usize = 20_000;
/// A pair of names with one `name_hash`: `crc32c(!1, name)` collides.
const COLLIDING: [&str; 2] = ["c59888d", "c1040060"];

#[test]
fn a_lookup_fetches_the_name_by_key() {
    assert_eq!(
        dir::name_hash(COLLIDING[0].as_bytes()),
        dir::name_hash(COLLIDING[1].as_bytes()),
        "fixture: the pair must collide"
    );

    let root = std::path::PathBuf::from(temp_path!("lookup-by-name-key"));
    let big = root.join("big");
    std::fs::create_dir_all(&big).unwrap();
    for i in 0..NAMES - COLLIDING.len() {
        std::fs::write(big.join(format!("entry_{i:05}")), b"").unwrap();
    }
    for name in COLLIDING {
        std::fs::write(big.join(name), name.as_bytes()).unwrap();
    }
    let image = root.with_extension("img");
    std::fs::File::create(&image)
        .and_then(|f| f.set_len(256 * 1024 * 1024))
        .unwrap();
    let out = oracle("mkfs.btrfs")
        .args(["-q", "-f", "--rootdir"])
        .arg(&root)
        .arg(&image)
        .output();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);

    let fs = Filesystem::mount(Arc::new(FileDevice::open(&image).unwrap())).expect("mount");
    let big = fs.lookup_path("/big").expect("/big");
    let listed = fs.read_dir(big.ino).expect("read_dir");
    assert_eq!(listed.len(), NAMES);

    let start = Instant::now();
    for entry in &listed {
        let inode = fs
            .lookup(big.ino, &entry.name)
            .unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(&entry.name)));
        assert_eq!(
            inode.ino,
            entry.ino,
            "{}",
            String::from_utf8_lossy(&entry.name)
        );
    }
    let elapsed = start.elapsed();
    eprintln!("{NAMES} lookups: {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(5),
        "{NAMES} lookups took {elapsed:?}: each is listing the directory"
    );

    for name in COLLIDING {
        let inode = fs.lookup(big.ino, name.as_bytes()).expect(name);
        let contents = fs.read_file(inode.ino).expect("read");
        assert_eq!(contents, name.as_bytes(), "{name} resolved to its twin");
    }
    for absent in ["entry_99999", "c0000000", "", ".", ".."] {
        assert!(
            fs.lookup(big.ino, absent.as_bytes()).is_err(),
            "{absent:?} is not in the directory"
        );
    }
    let file = fs.lookup(big.ino, b"entry_00000").unwrap();
    assert!(fs.lookup(file.ino, b"x").is_err(), "a file has no names");

    let _ = std::fs::remove_file(&image);
}
