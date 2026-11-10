//! `fs_btrfs_readlink` against THE KERNEL's `readlink`.
//!
//! The C-ABI tests in `tests/capi.rs` pin the contract's shape — the
//! length returned, the terminator, ERANGE with an untouched buffer —
//! against a target this suite already believes in. Believing it is the
//! trap: a reader that misparsed an inline symlink extent would agree
//! with a test written from the same reading. So here the expected
//! target is not ours at all. The in-kernel btrfs driver mounts the
//! fixture in the harness guest and `readlink(1)` reports every link on
//! it; each must come back through the C ABI byte for byte, with the
//! length the contract promises.
//!
//! `btrfs-rich` is built by `ln -s` through a real mount
//! (`test-disks/guest-build-images.sh`), so the links are the kernel's
//! own on both sides of the comparison.

use fs_btrfs::capi::*;
use fs_btrfs_test_support::{fixture, guest_kernel_report};
use std::ffi::{c_char, CStr, CString};

fn last_error() -> String {
    unsafe { CStr::from_ptr(fs_btrfs_last_error()) }
        .to_string_lossy()
        .into_owned()
}

#[test]
fn every_symlink_target_matches_what_the_kernel_reads() {
    let image = fixture("btrfs-rich.img");
    let image_str = image.to_str().expect("fixture path is UTF-8");
    let report = guest_kernel_report(image_str, "readlink oracle");

    let links: Vec<(&String, &String)> = report
        .iter()
        .filter(|((kind, _), _)| kind == "target")
        .map(|((_, path), target)| (path, target))
        .collect();
    assert!(
        !links.is_empty(),
        "the kernel reported no symlink on btrfs-rich; the fixture recipe makes one \
         (`ln -s inline.txt link-short`), so the report or the fixture is wrong"
    );

    let c_image = CString::new(image_str).unwrap();
    let fs = unsafe { fs_btrfs_mount(c_image.as_ptr()) };
    assert!(!fs.is_null(), "mount failed: {}", last_error());

    for (path, kernel_target) in links {
        let c_path = CString::new(format!("/{path}")).unwrap();
        let mut buf = [0 as c_char; 4096];
        let n = unsafe { fs_btrfs_readlink(fs, c_path.as_ptr(), buf.as_mut_ptr(), buf.len()) };
        assert!(n >= 0, "/{path}: readlink failed: {}", last_error());
        let ours = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_bytes();
        println!("[kernel vm] readlink /{path}: kernel {kernel_target:?}, driver returned {n}");
        assert_eq!(
            ours,
            kernel_target.as_bytes(),
            "/{path}: the driver's target differs from the kernel's"
        );
        assert_eq!(
            usize::try_from(n).unwrap(),
            kernel_target.len(),
            "/{path}: the return value is not the target length"
        );
    }

    unsafe { fs_btrfs_umount(fs) };
}
