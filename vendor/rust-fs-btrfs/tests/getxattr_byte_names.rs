//! A non-UTF-8 attribute name that `listxattr` returns reads back through
//! `getxattr` with the same bytes (#104).
//!
//! The checked-in xattr fixture's names are all UTF-8, so its round trip
//! holds with or without the fix. This test makes its own image instead:
//! a file carrying `user.caf\xe9\xff`, set by `setfattr` and copied in by
//! `mkfs.btrfs --rootdir`, which needs no mount and no root.
//!
//! Both tools run in the harness VM, against a scratch tree inside this
//! repository -- the only tree the guest can see -- so there is no host
//! btrfs-progs to be absent and nothing here skips. `setfattr` runs there
//! too: an attribute the guest cannot store fails this test and says so,
//! rather than reading as a pass.

use fs_btrfs::capi::*;
use fs_btrfs_test_support::{oracle, temp_path};
use std::ffi::{c_char, CStr, CString, OsStr};
use std::os::unix::ffi::OsStrExt;

const NAME: &[u8] = b"user.caf\xe9\xff";
const VALUE: &[u8] = b"read back by exact bytes";

/// An image whose `/f` carries [`NAME`] = [`VALUE`].
fn image() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("xattr-bytes"));
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("f");
    std::fs::write(&file, b"f").unwrap();
    let mut value_arg = b"0x".to_vec();
    value_arg.extend(VALUE.iter().flat_map(|b| format!("{b:02x}").into_bytes()));
    let set = oracle("setfattr")
        .arg("-n")
        .arg(OsStr::from_bytes(NAME))
        .arg("-v")
        .arg(OsStr::from_bytes(&value_arg))
        .arg(&file)
        .output();
    assert!(
        set.status.success(),
        "setfattr failed: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    let img = dir.join("img");
    std::fs::File::create(&img)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs")
        .args(["-f", "--rootdir"])
        .arg(&root)
        .arg(&img)
        .output();
    assert!(
        made.status.success(),
        "mkfs.btrfs failed: {}",
        String::from_utf8_lossy(&made.stderr)
    );
    img
}

fn last_error() -> String {
    let p = fs_btrfs_last_error();
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

#[test]
fn a_listed_non_utf8_name_reads_back_by_its_bytes() {
    let img = image();
    let img_c = CString::new(img.as_os_str().as_bytes()).unwrap();
    let fs = unsafe { fs_btrfs_mount(img_c.as_ptr()) };
    assert!(!fs.is_null(), "mount: {}", last_error());
    let path = CString::new("/f").unwrap();

    let needed = unsafe { fs_btrfs_listxattr(fs, path.as_ptr(), std::ptr::null_mut(), 0) };
    assert!(needed > 0, "{}", last_error());
    let mut list = vec![0u8; needed as usize];
    let got = unsafe {
        fs_btrfs_listxattr(
            fs,
            path.as_ptr(),
            list.as_mut_ptr().cast::<c_char>(),
            list.len(),
        )
    };
    assert_eq!(got, needed);
    let listed = list
        .split(|&b| b == 0)
        .find(|n| *n == NAME)
        .unwrap_or_else(|| {
            panic!(
                "listxattr did not return the name: {:?}",
                String::from_utf8_lossy(&list)
            )
        });

    // The exact bytes listxattr handed out, handed straight back.
    let name = CString::new(listed.to_vec()).unwrap();
    let mut buf = vec![0u8; 64];
    let n = unsafe {
        fs_btrfs_getxattr(
            fs,
            path.as_ptr(),
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    assert_eq!(n, VALUE.len() as i64, "getxattr: {}", last_error());
    assert_eq!(&buf[..VALUE.len()], VALUE);
    unsafe { fs_btrfs_umount(fs) };
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}
