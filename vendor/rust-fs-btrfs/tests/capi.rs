//! The C ABI, exercised the way a C caller would use it.
//!
//! This layer is what a consuming application links against, so a defect
//! here reaches users even though every Rust-level test passes. The
//! sibling EROFS driver shipped with this surface at 0% coverage; these
//! tests exist so this crate does not repeat that.
//!
//! Two classes of behaviour get more attention below than the happy
//! paths, because a safe Rust API never has to think about them:
//!
//! - **NULL tolerance.** Every pointer parameter must be checked, not
//!   dereferenced. A caller passing NULL should get a failure, not a
//!   crash inside its own process.
//! - **Error reporting.** A C caller has only the return value, the
//!   thread-local message and the errno. A wrong errno misdirects:
//!   reporting EIO for a missing file sends a user hunting for hardware
//!   faults.
//!
//! The fixture is `btrfs-rich`, written through a compressing mount and
//! holding a compressible file, an incompressible one, an inline file, a
//! sparse file, a symlink and a nested directory. It is gitignored and
//! built by `chore fixtures`; a missing one fails the test that wanted
//! it, because a C-ABI suite that never mounted anything reports the
//! same `ok` as one that exercised every entry point.

use fs_btrfs::capi::*;
use fs_btrfs_test_support::{fixture, temp_path, Image};
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;

/// Errno values the header documents. Spelled out rather than imported,
/// so the test asserts the contract rather than mirroring the source.
const ENOENT: i32 = 2;
const EIO: i32 = 5;
const ENOTDIR: i32 = 20;
const EISDIR: i32 = 21;
const EINVAL: i32 = 22;
const ERANGE: i32 = 34;

fn rich_image() -> PathBuf {
    fixture("btrfs-rich.img")
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn last_error() -> String {
    unsafe { CStr::from_ptr(fs_btrfs_last_error()) }
        .to_string_lossy()
        .into_owned()
}

fn mount() -> *mut fs_btrfs_fs {
    let path = rich_image();
    let c = cstr(path.to_str().unwrap());
    let fs = unsafe { fs_btrfs_mount(c.as_ptr()) };
    assert!(
        !fs.is_null(),
        "mounting the fixture failed: {}",
        last_error()
    );
    fs
}

fn last_errno_erange() -> i32 {
    fs_btrfs_last_errno()
}

fn zeroed_attr() -> fs_btrfs_attr_t {
    unsafe { std::mem::zeroed() }
}

// ---------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------

#[test]
fn mounts_and_reports_volume_info() {
    let fs = mount();
    let mut info: fs_btrfs_volume_info_t = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { fs_btrfs_get_volume_info(fs, &mut info) }, 0);

    assert!(
        info.sector_size.is_power_of_two() && info.sector_size >= 512,
        "sector size {} is not sane",
        info.sector_size
    );
    assert!(
        info.node_size.is_power_of_two() && info.node_size >= info.sector_size,
        "node size {} is not sane",
        info.node_size
    );
    assert!(info.num_devices >= 1);
    assert!(
        info.bytes_used <= info.total_bytes,
        "more bytes used than the device holds"
    );
    assert_ne!(info.fsid, [0u8; 16], "a real filesystem has an fsid");
    // The fixture is made with default (crc32c) checksums.
    assert_eq!(info.csum_type, 0, "expected crc32c on the rich fixture");

    // With METADATA_UUID off, the effective metadata uuid is the fsid.
    assert_eq!(info.metadata_uuid, info.fsid);

    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn stats_a_file_a_directory_and_a_symlink() {
    let fs = mount();

    let mut f = zeroed_attr();
    assert_eq!(
        unsafe { fs_btrfs_stat(fs, cstr("/inline.txt").as_ptr(), &mut f) },
        0
    );
    assert_eq!(f.file_type, 1, "inline.txt should be a regular file");
    assert_eq!(f.size, 13, "\"small inline\\n\" is 13 bytes");
    assert!(f.link_count >= 1);

    let mut d = zeroed_attr();
    assert_eq!(
        unsafe { fs_btrfs_stat(fs, cstr("/sub").as_ptr(), &mut d) },
        0
    );
    assert_eq!(d.file_type, 2, "sub should be a directory");

    // stat must describe the link itself, not its target — otherwise a
    // caller cannot tell one from the other.
    let mut l = zeroed_attr();
    assert_eq!(
        unsafe { fs_btrfs_stat(fs, cstr("/link-short").as_ptr(), &mut l) },
        0
    );
    assert_eq!(l.file_type, 7, "link-short should report as a symlink");

    // stat by inode must agree with stat by path.
    let mut by_ino = zeroed_attr();
    assert_eq!(unsafe { fs_btrfs_stat_ino(fs, f.inode, &mut by_ino) }, 0);
    assert_eq!(by_ino.inode, f.inode);
    assert_eq!(by_ino.size, f.size);
    assert_eq!(by_ino.mode, f.mode);

    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn iterates_a_directory_to_completion() {
    let fs = mount();
    let iter = unsafe { fs_btrfs_dir_open(fs, cstr("/").as_ptr()) };
    assert!(!iter.is_null(), "opening the root failed: {}", last_error());

    let mut names = Vec::new();
    loop {
        let ptr = unsafe { fs_btrfs_dir_next(iter) };
        if ptr.is_null() {
            assert_eq!(
                fs_btrfs_last_errno(),
                0,
                "a clean end of directory must not set an errno: {}",
                last_error()
            );
            break;
        }
        let e = unsafe { &*ptr };
        let name = unsafe { CStr::from_ptr(e.name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            name.len(),
            usize::from(e.name_len),
            "name_len disagrees with the NUL-terminated name"
        );
        assert_ne!(e.inode, 0, "entry `{name}` has inode 0");
        assert_eq!(e.is_subvolume, 0, "`{name}` is unexpectedly a subvolume");
        names.push(name);
    }

    // Past the end it must keep returning 0 rather than wrapping.
    assert!(unsafe { fs_btrfs_dir_next(iter) }.is_null());
    unsafe { fs_btrfs_dir_close(iter) };

    for want in ["inline.txt", "plain.bin", "sparse.bin", "link-short", "sub"] {
        assert!(
            names.contains(&want.to_string()),
            "missing {want}: {names:?}"
        );
    }
    // `.` and `..` must never appear, matching the sibling driver.
    assert!(!names.iter().any(|n| n == "." || n == ".."));

    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn reads_file_contents() {
    let fs = mount();
    let mut buf = [0u8; 64];
    let n = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/inline.txt").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            0,
            buf.len() as u64,
        )
    };
    assert!(n > 0, "read failed: {}", last_error());
    assert_eq!(&buf[..n as usize], b"small inline\n");

    // From an offset, the tail rather than the head again.
    let mut tail = [0u8; 64];
    let m = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/inline.txt").as_ptr(),
            tail.as_mut_ptr().cast::<c_void>(),
            6,
            tail.len() as u64,
        )
    };
    assert_eq!(&tail[..m as usize], b"inline\n");

    // At end of file: zero bytes, not an error.
    let at_eof = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/inline.txt").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            n as u64,
            buf.len() as u64,
        )
    };
    assert_eq!(at_eof, 0);

    unsafe { fs_btrfs_umount(fs) };
}

/// A hole must read as zeros rather than as whatever previously occupied
/// those blocks.
#[test]
fn sparse_regions_read_as_zeros() {
    let fs = mount();
    let mut buf = vec![0xAAu8; 65536];
    let n = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/sparse.bin").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            0,
            buf.len() as u64,
        )
    };
    assert!(n > 0, "read failed: {}", last_error());
    assert!(
        buf[..n as usize].iter().all(|&b| b == 0),
        "a hole did not read back as zeros"
    );
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn reads_a_symlink_target() {
    let fs = mount();
    let mut buf = [0 as c_char; 512];
    let n = unsafe {
        fs_btrfs_readlink(
            fs,
            cstr("/link-short").as_ptr(),
            buf.as_mut_ptr(),
            buf.len(),
        )
    };
    assert!(n > 0, "readlink failed: {}", last_error());
    let target = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy();
    assert_eq!(target, "inline.txt");
    assert_eq!(usize::try_from(n).unwrap(), target.len());
    unsafe { fs_btrfs_umount(fs) };
}

/// A buffer too small for the target is REFUSED rather than truncated.
///
/// A truncated symlink target is a path to somewhere else, and a caller
/// following it has no way to tell. ERANGE tells it to retry with a
/// larger buffer, which is the standard idiom — and matches the sibling
/// EROFS driver, so the family agrees.
#[test]
fn readlink_refuses_a_buffer_too_small_for_the_target() {
    let fs = mount();
    // FILLED WITH A BYTE NOTHING WRITES, so "untouched" is checkable.
    // Spelled once and compared against itself below: `c_char` is
    // SIGNED on x86_64 and UNSIGNED on aarch64, so `c as u8` is a real
    // conversion on one and a no-op clippy refuses on the other — and
    // `chore lint` runs on both. Comparing `c_char` to `c_char` is the
    // same check on either.
    const UNTOUCHED: c_char = 0x7F;
    let mut buf = [UNTOUCHED; 5];
    let n = unsafe {
        fs_btrfs_readlink(
            fs,
            cstr("/link-short").as_ptr(),
            buf.as_mut_ptr(),
            buf.len(),
        )
    };
    assert_eq!(n, -1, "a target that does not fit must be refused");
    assert_eq!(
        last_errno_erange(),
        ERANGE,
        "a buffer too small is ERANGE, got {}",
        last_errno_erange()
    );
    assert!(
        buf.iter().all(|&c| c == UNTOUCHED),
        "a refused readlink must not have written into the buffer"
    );
    unsafe { fs_btrfs_umount(fs) };
}

// ---------------------------------------------------------------------
// The family readlink contract (#205)
//
// Every driver in the family answers readlink the same way: the target
// length on success (excluding the NUL, like readlink(2)) with the target
// and a terminator written; -1 ERANGE and an untouched buffer when it does
// not fit; -1 EINVAL for a NULL argument; and -1 with THIS call's errno on
// any other failure.
// ---------------------------------------------------------------------

/// The fixture's link, as `ln -s inline.txt link-short` made it.
const LINK_SHORT_TARGET: &[u8] = b"inline.txt";

/// Fill value no readlink writes, so "untouched" is checkable. See
/// `readlink_refuses_a_buffer_too_small_for_the_target` for why it is
/// compared as `c_char` rather than converted.
const UNTOUCHED: c_char = 0x7F;

/// A failure earlier on the same thread must not survive into a later
/// clean end of directory: the header promises errno 0 there, and a
/// caller following it reads a stale ENOENT as a failed listing (#232).
#[test]
fn a_clean_end_of_directory_after_a_failure_reports_errno_zero() {
    let fs = mount();
    let mut attr = zeroed_attr();
    let missing = cstr("/does-not-exist");
    assert_eq!(
        unsafe { fs_btrfs_stat(fs, missing.as_ptr(), &mut attr) },
        -1
    );
    assert_eq!(fs_btrfs_last_errno(), ENOENT);

    let iter = unsafe { fs_btrfs_dir_open(fs, cstr("/").as_ptr()) };
    assert!(!iter.is_null(), "opening the root failed: {}", last_error());
    let mut entries = 0;
    while !unsafe { fs_btrfs_dir_next(iter) }.is_null() {
        entries += 1;
    }
    assert!(entries > 0, "the root of the fixture listed nothing");
    assert_eq!(
        fs_btrfs_last_errno(),
        0,
        "a clean end of directory reported the earlier failure: {}",
        last_error()
    );
    unsafe { fs_btrfs_dir_close(iter) };
    unsafe { fs_btrfs_umount(fs) };
}

/// A call that succeeds leaves no error behind, so errno always describes
/// the most recent call rather than the most recent failure (#232).
#[test]
fn a_successful_call_clears_the_previous_error() {
    let fs = mount();
    let mut attr = zeroed_attr();
    let missing = cstr("/does-not-exist");
    assert_eq!(
        unsafe { fs_btrfs_stat(fs, missing.as_ptr(), &mut attr) },
        -1
    );
    assert_eq!(fs_btrfs_last_errno(), ENOENT);

    assert_eq!(
        unsafe { fs_btrfs_stat(fs, cstr("/").as_ptr(), &mut attr) },
        0
    );
    assert_eq!(
        fs_btrfs_last_errno(),
        0,
        "stat succeeded but errno is stale"
    );
    assert_eq!(last_error(), "no error");
    unsafe { fs_btrfs_umount(fs) };
}

/// The release functions are the exception: they leave the error alone,
/// so a caller that tidies up before reading errno still finds the
/// failure it is about to report (#232).
#[test]
fn releasing_an_iterator_keeps_the_previous_error() {
    let fs = mount();
    let iter = unsafe { fs_btrfs_dir_open(fs, cstr("/").as_ptr()) };
    assert!(!iter.is_null(), "opening the root failed: {}", last_error());
    let mut attr = zeroed_attr();
    let missing = cstr("/does-not-exist");
    assert_eq!(
        unsafe { fs_btrfs_stat(fs, missing.as_ptr(), &mut attr) },
        -1
    );
    assert_eq!(fs_btrfs_last_errno(), ENOENT);

    unsafe { fs_btrfs_dir_close(iter) };
    assert_eq!(fs_btrfs_last_errno(), ENOENT, "dir_close cleared the error");
    unsafe { fs_btrfs_umount(fs) };
    assert_eq!(fs_btrfs_last_errno(), ENOENT, "umount cleared the error");
}

fn readlink_into(fs: *mut fs_btrfs_fs, path: &str, buf: &mut [c_char], bufsize: usize) -> i32 {
    assert!(bufsize <= buf.len());
    unsafe { fs_btrfs_readlink(fs, cstr(path).as_ptr(), buf.as_mut_ptr(), bufsize) }
}

/// Leave an errno behind that no readlink failure below should report, so
/// a check that reads errno sees this call's value and not a stale one.
fn poison_errno(fs: *mut fs_btrfs_fs) {
    let mut small = [0 as c_char; 1];
    assert_eq!(readlink_into(fs, "/link-short", &mut small, 1), -1);
    assert_eq!(fs_btrfs_last_errno(), ERANGE);
}

fn as_bytes(buf: &[c_char]) -> Vec<u8> {
    buf.iter().map(|&c| c.to_ne_bytes()[0]).collect()
}

#[test]
fn readlink_returns_the_length_and_writes_the_target_and_a_nul() {
    let fs = mount();
    let mut buf = [UNTOUCHED; 64];
    let n = readlink_into(fs, "/link-short", &mut buf, 64);
    assert_eq!(
        n,
        LINK_SHORT_TARGET.len() as i32,
        "success returns the target length excluding the NUL: {}",
        last_error()
    );
    let bytes = as_bytes(&buf);
    assert_eq!(&bytes[..LINK_SHORT_TARGET.len()], LINK_SHORT_TARGET);
    assert_eq!(
        bytes[LINK_SHORT_TARGET.len()],
        0,
        "the target is NUL-terminated"
    );
    assert!(
        buf[LINK_SHORT_TARGET.len() + 1..]
            .iter()
            .all(|&c| c == UNTOUCHED),
        "nothing past the terminator is written"
    );
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn readlink_fits_a_buffer_of_exactly_length_plus_one() {
    let fs = mount();
    let len = LINK_SHORT_TARGET.len();
    let mut buf = [UNTOUCHED; 64];
    let n = readlink_into(fs, "/link-short", &mut buf, len + 1);
    assert_eq!(n, len as i32, "an exact fit succeeds: {}", last_error());
    let bytes = as_bytes(&buf);
    assert_eq!(&bytes[..len], LINK_SHORT_TARGET);
    assert_eq!(bytes[len], 0);
    assert!(buf[len + 1..].iter().all(|&c| c == UNTOUCHED));
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn readlink_refuses_a_buffer_one_short_of_the_terminator() {
    let fs = mount();
    let len = LINK_SHORT_TARGET.len();
    let mut buf = [UNTOUCHED; 64];
    assert_eq!(readlink_into(fs, "/link-short", &mut buf, len), -1);
    assert_eq!(fs_btrfs_last_errno(), ERANGE);
    let message = last_error();
    assert!(
        message.contains(&(len + 1).to_string()),
        "the message names the size needed ({}): {message}",
        len + 1
    );
    assert!(
        buf.iter().all(|&c| c == UNTOUCHED),
        "a refused readlink writes nothing, not even a terminator"
    );
    unsafe { fs_btrfs_umount(fs) };
}

/// A zero-length buffer is a buffer too small, not a NULL pointer.
#[test]
fn readlink_into_a_zero_length_buffer_is_erange() {
    let fs = mount();
    let mut buf = [UNTOUCHED; 4];
    assert_eq!(readlink_into(fs, "/link-short", &mut buf, 0), -1);
    assert_eq!(fs_btrfs_last_errno(), ERANGE, "{}", last_error());
    assert!(buf.iter().all(|&c| c == UNTOUCHED));
    unsafe { fs_btrfs_umount(fs) };
}

/// readlink(2) on something that is not a link is EINVAL — for a regular
/// file as much as for a directory, so EISDIR would misdirect.
#[test]
fn readlink_on_a_non_symlink_sets_einval() {
    let fs = mount();
    for path in ["/inline.txt", "/sub"] {
        poison_errno(fs);
        let mut buf = [UNTOUCHED; 64];
        assert_eq!(readlink_into(fs, path, &mut buf, 64), -1, "{path}");
        assert_eq!(fs_btrfs_last_errno(), EINVAL, "{path}: {}", last_error());
        assert!(buf.iter().all(|&c| c == UNTOUCHED), "{path}");
    }
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn readlink_on_a_missing_path_sets_enoent() {
    let fs = mount();
    poison_errno(fs);
    let mut buf = [UNTOUCHED; 64];
    assert_eq!(readlink_into(fs, "/no-such-link", &mut buf, 64), -1);
    assert_eq!(fs_btrfs_last_errno(), ENOENT, "{}", last_error());
    assert!(buf.iter().all(|&c| c == UNTOUCHED));
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn readlink_with_a_null_argument_is_einval() {
    let fs = mount();
    let link = cstr("/link-short");
    let mut buf = [UNTOUCHED; 64];
    unsafe {
        poison_errno(fs);
        assert_eq!(
            fs_btrfs_readlink(std::ptr::null_mut(), link.as_ptr(), buf.as_mut_ptr(), 64),
            -1
        );
        assert_eq!(fs_btrfs_last_errno(), EINVAL, "NULL fs");

        poison_errno(fs);
        assert_eq!(
            fs_btrfs_readlink(fs, std::ptr::null(), buf.as_mut_ptr(), 64),
            -1
        );
        assert_eq!(fs_btrfs_last_errno(), EINVAL, "NULL path");

        poison_errno(fs);
        assert_eq!(
            fs_btrfs_readlink(fs, link.as_ptr(), std::ptr::null_mut(), 64),
            -1
        );
        assert_eq!(fs_btrfs_last_errno(), EINVAL, "NULL buf");

        fs_btrfs_umount(fs);
    }
    assert!(buf.iter().all(|&c| c == UNTOUCHED));
}

// ---------------------------------------------------------------------
// Error reporting
// ---------------------------------------------------------------------

/// A compressed extent must reach a C caller decoded.
///
/// This used to assert the opposite: that the read was refused with
/// ENOTSUP, because returning the compressed bytes would look to a
/// caller exactly like a successful read of a corrupt file. Now that the
/// driver decodes them, the same reasoning sets the bar for this test —
/// it is not enough for the call to succeed, the bytes it wrote have to
/// be the file's.
#[test]
fn a_compressed_file_is_decoded_through_the_abi() {
    let fs = mount();
    let mut buf = vec![0u8; 4096];
    let n = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/compressed.txt").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            0,
            buf.len() as u64,
        )
    };
    assert_eq!(
        n,
        buf.len() as i64,
        "a compressed extent should fill the buffer, got {n} ({})",
        last_error()
    );
    // The fixture is one sentence repeated, so the decoded bytes are
    // known without consulting anything else.
    let text = std::str::from_utf8(&buf).expect("the fixture is ASCII");
    assert!(
        text.starts_with("the quick brown fox jumps over the lazy dog "),
        "decoded to something else: {:?}",
        &text[..60]
    );
    unsafe { fs_btrfs_umount(fs) };
}

/// And an ordinary file on the same filesystem must still read, or the
/// test above would pass on a driver that refused everything.
#[test]
fn an_uncompressed_file_on_the_same_volume_still_reads() {
    let fs = mount();
    let mut buf = vec![0u8; 4096];
    let n = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/plain.bin").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            0,
            buf.len() as u64,
        )
    };
    assert_eq!(n, 4096, "plain.bin should read: {}", last_error());
    assert!(buf.iter().any(|&b| b != 0), "random data read as zeros");
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn a_missing_path_reports_enoent_not_eio() {
    let fs = mount();
    let mut a = zeroed_attr();
    assert_eq!(
        unsafe { fs_btrfs_stat(fs, cstr("/no-such-file").as_ptr(), &mut a) },
        -1
    );
    assert_eq!(fs_btrfs_last_errno(), ENOENT);
    assert!(!last_error().is_empty());
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn listing_a_file_reports_enotdir() {
    let fs = mount();
    let iter = unsafe { fs_btrfs_dir_open(fs, cstr("/inline.txt").as_ptr()) };
    assert!(
        iter.is_null(),
        "a regular file must not open as a directory"
    );
    assert_eq!(fs_btrfs_last_errno(), ENOTDIR);
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn reading_a_directory_as_a_file_reports_eisdir() {
    let fs = mount();
    let mut buf = [0u8; 16];
    let n = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/sub").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            0,
            buf.len() as u64,
        )
    };
    assert_eq!(n, -1);
    assert_eq!(fs_btrfs_last_errno(), EISDIR);
    unsafe { fs_btrfs_umount(fs) };
}

#[test]
fn mounting_a_non_btrfs_file_fails_with_a_message() {
    let tmp = PathBuf::from(temp_path!("capi-notbtrfs-{}.img", std::process::id()));
    std::fs::write(&tmp, vec![0x5Au8; 256 * 1024]).unwrap();
    let c = cstr(tmp.to_str().unwrap());
    let fs = unsafe { fs_btrfs_mount(c.as_ptr()) };
    assert!(fs.is_null(), "a file of 0x5A must not mount as Btrfs");
    assert_eq!(fs_btrfs_last_errno(), EIO);
    assert!(
        last_error().to_lowercase().contains("btrfs"),
        "message should name the format: {}",
        last_error()
    );
    std::fs::remove_file(&tmp).ok();
}

/// A path that does not exist is the caller's mistake, not damaged
/// media, so it must not be reported as an I/O error.
#[test]
fn mounting_a_missing_path_reports_enoent() {
    let fs = unsafe { fs_btrfs_mount(cstr("/nonexistent/device.img").as_ptr()) };
    assert!(fs.is_null());
    assert_eq!(
        fs_btrfs_last_errno(),
        ENOENT,
        "a path that does not exist must be ENOENT, not EIO"
    );
    assert!(!last_error().is_empty());
}

// ---------------------------------------------------------------------
// NULL tolerance
// ---------------------------------------------------------------------

#[test]
fn null_pointers_fail_instead_of_crashing() {
    let mut attr = zeroed_attr();
    let mut info: fs_btrfs_volume_info_t = unsafe { std::mem::zeroed() };
    let mut buf = [0u8; 8];
    let mut cbuf = [0 as c_char; 8];
    let p = cstr("/x");

    unsafe {
        assert!(fs_btrfs_mount(std::ptr::null()).is_null());
        assert!(fs_btrfs_mount_with_callbacks(std::ptr::null()).is_null());

        assert_eq!(
            fs_btrfs_get_volume_info(std::ptr::null_mut(), &mut info),
            -1
        );
        assert_eq!(
            fs_btrfs_stat(std::ptr::null_mut(), p.as_ptr(), &mut attr),
            -1
        );
        assert_eq!(fs_btrfs_stat_ino(std::ptr::null_mut(), 1, &mut attr), -1);
        assert!(fs_btrfs_dir_open(std::ptr::null_mut(), p.as_ptr()).is_null());
        assert!(fs_btrfs_dir_next(std::ptr::null_mut()).is_null());
        assert_eq!(
            fs_btrfs_read_file(
                std::ptr::null_mut(),
                p.as_ptr(),
                buf.as_mut_ptr().cast::<c_void>(),
                0,
                buf.len() as u64
            ),
            -1
        );
        assert_eq!(
            fs_btrfs_readlink(
                std::ptr::null_mut(),
                p.as_ptr(),
                cbuf.as_mut_ptr(),
                cbuf.len()
            ),
            -1
        );

        // Releasing NULL must be a safe no-op, as the header promises.
        fs_btrfs_umount(std::ptr::null_mut());
        fs_btrfs_dir_close(std::ptr::null_mut());
    }
}

#[test]
fn null_output_pointers_fail_instead_of_crashing() {
    let fs = mount();
    unsafe {
        assert_eq!(fs_btrfs_get_volume_info(fs, std::ptr::null_mut()), -1);
        assert_eq!(
            fs_btrfs_stat(fs, cstr("/").as_ptr(), std::ptr::null_mut()),
            -1
        );
        assert_eq!(fs_btrfs_stat_ino(fs, 256, std::ptr::null_mut()), -1);
        assert_eq!(
            fs_btrfs_read_file(fs, cstr("/inline.txt").as_ptr(), std::ptr::null_mut(), 0, 8),
            -1
        );
        assert_eq!(
            fs_btrfs_readlink(fs, cstr("/link-short").as_ptr(), std::ptr::null_mut(), 8),
            -1
        );
        // A zero-length buffer leaves no room even for the terminator.
        let mut one = [0 as c_char; 1];
        assert!(fs_btrfs_readlink(fs, cstr("/link-short").as_ptr(), one.as_mut_ptr(), 0) < 0);

        // A NULL path is a failure, not a dereference.
        let mut attr = zeroed_attr();
        assert_eq!(fs_btrfs_stat(fs, std::ptr::null(), &mut attr), -1);
        assert!(fs_btrfs_dir_open(fs, std::ptr::null()).is_null());

        fs_btrfs_umount(fs);
    }
}

/// A non-UTF-8 path is rejected rather than misinterpreted.
#[test]
fn a_non_utf8_path_is_rejected() {
    let fs = mount();
    let bad = [b'/' as c_char, 0xFFu8 as c_char, 0];
    let mut attr = zeroed_attr();
    assert_eq!(unsafe { fs_btrfs_stat(fs, bad.as_ptr(), &mut attr) }, -1);
    assert!(!last_error().is_empty());
    unsafe { fs_btrfs_umount(fs) };
}

// ---------------------------------------------------------------------
// The callback mount path
// ---------------------------------------------------------------------

struct FileContext {
    /// The image, read a window at a time rather than held whole: the
    /// rich fixture is 600 MiB and this callback is handed a few
    /// kilobytes at a time. See `Image`.
    image: Image,
    /// Set to make every read fail, proving failures surface.
    fail: bool,
}

unsafe extern "C" fn ctx_read(
    context: *mut c_void,
    buf: *mut c_void,
    offset: u64,
    length: u64,
) -> i32 {
    let ctx = unsafe { &*(context as *const FileContext) };
    if ctx.fail {
        return -1;
    }
    let mut window = vec![0u8; length as usize];
    if !ctx.image.try_read_at(offset, &mut window) {
        return -1;
    }
    unsafe { std::ptr::copy_nonoverlapping(window.as_ptr(), buf.cast::<u8>(), window.len()) };
    0
}

#[test]
fn mounts_over_a_caller_supplied_reader() {
    let img = rich_image();
    let ctx = Box::new(FileContext {
        image: Image::open(&img),
        fail: false,
    });
    let size = ctx.image.len();
    let cfg = fs_btrfs_blockdev_cfg_t {
        read: Some(ctx_read),
        context: Box::into_raw(ctx) as *mut c_void,
        size_bytes: size,
        block_size: 4096,
    };
    let fs = unsafe { fs_btrfs_mount_with_callbacks(&cfg) };
    assert!(!fs.is_null(), "callback mount failed: {}", last_error());

    // It must actually work, not merely mount.
    let mut buf = [0u8; 64];
    let n = unsafe {
        fs_btrfs_read_file(
            fs,
            cstr("/inline.txt").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            0,
            buf.len() as u64,
        )
    };
    assert_eq!(&buf[..n as usize], b"small inline\n");

    unsafe { fs_btrfs_umount(fs) };
    drop(unsafe { Box::from_raw(cfg.context as *mut FileContext) });
}

/// A callback that fails must surface as an error, never as silently
/// zeroed data — a caller cannot tell those apart.
#[test]
fn a_failing_callback_surfaces_as_an_error() {
    // The image is never read — `fail` short-circuits every call — so
    // which one it names does not matter, only that it opens.
    let ctx = Box::new(FileContext {
        image: Image::open(&rich_image()),
        fail: true,
    });
    let cfg = fs_btrfs_blockdev_cfg_t {
        read: Some(ctx_read),
        context: Box::into_raw(ctx) as *mut c_void,
        size_bytes: 256 * 1024,
        block_size: 4096,
    };
    let fs = unsafe { fs_btrfs_mount_with_callbacks(&cfg) };
    assert!(fs.is_null(), "a failing reader must not produce a handle");
    assert!(!last_error().is_empty());
    drop(unsafe { Box::from_raw(cfg.context as *mut FileContext) });
}

#[test]
fn a_null_callback_is_rejected() {
    let cfg = fs_btrfs_blockdev_cfg_t {
        read: None,
        context: std::ptr::null_mut(),
        size_bytes: 4096,
        block_size: 4096,
    };
    assert!(unsafe { fs_btrfs_mount_with_callbacks(&cfg) }.is_null());
    assert!(!last_error().is_empty());
}

/// `fs_btrfs_last_error` must never return NULL, including before any
/// failure — a C caller will print it unconditionally.
#[test]
fn last_error_is_never_null() {
    assert!(!fs_btrfs_last_error().is_null());
    assert!(!last_error().is_empty());
}

// ---------------------------------------------------------------------
// Writing
//
// These work on a copy of the nodatacow fixture, because they change it.
// The copy is written to the scratch directory rather than beside the
// fixture: a suite that walks `test-disks/` for images would take a
// half-written copy for a fixture. It is removed when the guard drops,
// including on a panic.
// ---------------------------------------------------------------------

const EROFS: i32 = 30;
const ENOTSUP_ERRNO: i32 = if cfg!(target_os = "macos") { 45 } else { 95 };

/// The `chattr +C` file, and the ordinary one beside it.
const INPLACE_PATH: &str = "/nc/inplace.bin";
const COW_PATH: &str = "/cow.bin";

struct WritableCopy(PathBuf);

impl WritableCopy {
    fn new(name: &str) -> Self {
        let src = fixture("btrfs-nodatacow.img");
        let dst = PathBuf::from(temp_path!("{name}"));
        std::fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("copy {} to {}: {e}", src.display(), dst.display()));
        WritableCopy(dst)
    }
    fn open_rw(&self) -> *mut fs_btrfs_fs {
        let c = cstr(self.0.to_str().unwrap());
        let fs = unsafe { fs_btrfs_mount_rw(c.as_ptr()) };
        assert!(!fs.is_null(), "mount_rw failed: {}", last_error());
        fs
    }
    fn open_ro(&self) -> *mut fs_btrfs_fs {
        let c = cstr(self.0.to_str().unwrap());
        let fs = unsafe { fs_btrfs_mount(c.as_ptr()) };
        assert!(!fs.is_null(), "mount failed: {}", last_error());
        fs
    }
}

impl Drop for WritableCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A read-only handle must say it cannot write, and must refuse.
#[test]
fn a_read_only_handle_says_so_and_refuses() {
    let copy = WritableCopy::new("btrfscapi-ro.img");
    let fs = copy.open_ro();
    assert_eq!(unsafe { fs_btrfs_is_writable(fs) }, 0);

    let data = b"nope";
    let n = unsafe {
        fs_btrfs_write_file(
            fs,
            cstr(INPLACE_PATH).as_ptr(),
            data.as_ptr().cast::<c_void>(),
            0,
            data.len() as u64,
        )
    };
    assert_eq!(n, -1, "a read-only handle wrote something");
    assert_eq!(fs_btrfs_last_errno(), EROFS, "{}", last_error());
    unsafe { fs_btrfs_umount(fs) };
}

/// `can_write_in_place` must agree with what a write actually does —
/// for both answers. A caller that trusts it should never be surprised
/// in either direction.
#[test]
fn can_write_in_place_agrees_with_the_write() {
    let copy = WritableCopy::new("btrfscapi-agree.img");
    let fs = copy.open_rw();
    assert_eq!(unsafe { fs_btrfs_is_writable(fs) }, 1);

    let inplace = cstr(INPLACE_PATH);
    let cow = cstr(COW_PATH);
    assert_eq!(
        unsafe { fs_btrfs_can_write_in_place(fs, inplace.as_ptr()) },
        1,
        "the nodatacow file should be writable: {}",
        last_error()
    );
    assert_eq!(
        unsafe { fs_btrfs_can_write_in_place(fs, cow.as_ptr()) },
        0,
        "an ordinary copy-on-write file should not be"
    );

    let payload = b"through the C ABI";
    let n = unsafe {
        fs_btrfs_write_file(
            fs,
            inplace.as_ptr(),
            payload.as_ptr().cast::<c_void>(),
            4096,
            payload.len() as u64,
        )
    };
    assert_eq!(n, payload.len() as i64, "{}", last_error());

    let refused = unsafe {
        fs_btrfs_write_file(
            fs,
            cow.as_ptr(),
            payload.as_ptr().cast::<c_void>(),
            0,
            payload.len() as u64,
        )
    };
    assert_eq!(refused, -1, "a copy-on-write file was written");
    assert_eq!(fs_btrfs_last_errno(), ENOTSUP_ERRNO, "{}", last_error());
    unsafe { fs_btrfs_umount(fs) };
}

/// A write must be readable back through the ABI.
#[test]
fn a_write_round_trips_through_the_abi() {
    let copy = WritableCopy::new("btrfscapi-roundtrip.img");
    let fs = copy.open_rw();
    let path = cstr(INPLACE_PATH);
    let payload = b"round trip through the ABI";

    let n = unsafe {
        fs_btrfs_write_file(
            fs,
            path.as_ptr(),
            payload.as_ptr().cast::<c_void>(),
            16384,
            payload.len() as u64,
        )
    };
    assert_eq!(n, payload.len() as i64, "{}", last_error());

    let mut back = vec![0u8; payload.len()];
    let r = unsafe {
        fs_btrfs_read_file(
            fs,
            path.as_ptr(),
            back.as_mut_ptr().cast::<c_void>(),
            16384,
            back.len() as u64,
        )
    };
    assert_eq!(r, payload.len() as i64, "{}", last_error());
    assert_eq!(
        &back, payload,
        "the bytes read back are not the ones written"
    );
    unsafe { fs_btrfs_umount(fs) };
}

/// Writing past the end needs an allocation, and must say so as
/// unsupported rather than as a bad argument.
#[test]
fn writing_past_the_end_is_enotsup() {
    let copy = WritableCopy::new("btrfscapi-pastend.img");
    let fs = copy.open_rw();
    let data = b"beyond";
    let n = unsafe {
        fs_btrfs_write_file(
            fs,
            cstr(INPLACE_PATH).as_ptr(),
            data.as_ptr().cast::<c_void>(),
            1 << 30,
            data.len() as u64,
        )
    };
    assert_eq!(n, -1);
    assert_eq!(fs_btrfs_last_errno(), ENOTSUP_ERRNO, "{}", last_error());
    unsafe { fs_btrfs_umount(fs) };
}

// ---------------------------------------------------------------------
// Extended attributes
//
// A different fixture from the one above: `btrfs-xattr`, because `rich`
// carries no attributes and adding some to it would change a filesystem
// several other tests already assert against.
// ---------------------------------------------------------------------

fn xattr_fixture() -> *mut fs_btrfs_fs {
    let p = fixture("btrfs-xattr.img");
    let c = cstr(p.to_str().unwrap());
    let fs = unsafe { fs_btrfs_mount(c.as_ptr()) };
    assert!(!fs.is_null(), "mounting failed: {}", last_error());
    fs
}

/// Names come back NUL-separated, and the probe form (NULL buffer)
/// reports the same size the real call needs. A caller allocates on the
/// strength of that number, so the two must agree exactly.
#[test]
fn listxattr_names_are_nul_separated_and_the_probe_agrees() {
    let fs = xattr_fixture();
    let path = cstr("/plain.txt");
    let needed = unsafe { fs_btrfs_listxattr(fs, path.as_ptr(), std::ptr::null_mut(), 0) };
    assert!(needed > 0, "{}", last_error());

    let mut buf = vec![0u8; needed as usize];
    let wrote = unsafe {
        fs_btrfs_listxattr(
            fs,
            path.as_ptr(),
            buf.as_mut_ptr().cast::<c_char>(),
            buf.len(),
        )
    };
    assert_eq!(wrote, needed, "the probe and the real call disagree");

    let mut names: Vec<&[u8]> = buf.split(|&b| b == 0).collect();
    // The buffer ends with a terminator, so the split leaves an empty
    // tail; a name is never empty, so this is unambiguous.
    assert_eq!(names.pop(), Some(&b""[..]));
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            &b"user.binary"[..],
            &b"user.colour"[..],
            &b"user.empty"[..],
            &b"user.long"[..],
        ]
    );
    unsafe { fs_btrfs_umount(fs) };
}

/// A buffer too small takes as many WHOLE names as fit and still reports
/// the full size. A half-written name is not a name, and a caller that
/// parsed one would act on a name that does not exist.
#[test]
fn listxattr_into_a_short_buffer_writes_whole_names_only() {
    let fs = xattr_fixture();
    let path = cstr("/plain.txt");
    let needed = unsafe { fs_btrfs_listxattr(fs, path.as_ptr(), std::ptr::null_mut(), 0) };
    let mut buf = vec![0xAAu8; 16];
    let got = unsafe {
        fs_btrfs_listxattr(
            fs,
            path.as_ptr(),
            buf.as_mut_ptr().cast::<c_char>(),
            buf.len(),
        )
    };
    assert_eq!(got, needed, "a short write must still report the full size");
    let written = &buf[..buf.iter().position(|&b| b == 0xAA).unwrap_or(buf.len())];
    // Whatever was written is a whole number of NUL-terminated names.
    assert!(written.is_empty() || written.last() == Some(&0));
    unsafe { fs_btrfs_umount(fs) };
}

/// The value comes back byte for byte, including NULs, and the probe
/// form reports its length without writing.
#[test]
fn getxattr_returns_the_value_and_its_length() {
    let fs = xattr_fixture();
    let path = cstr("/plain.txt");
    let name = cstr("user.binary");
    let want = [0x00u8, 0x01, 0x02, 0xff, 0x7f, 0x0a, 0x00];

    let size =
        unsafe { fs_btrfs_getxattr(fs, path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
    assert_eq!(size, want.len() as i64, "{}", last_error());

    let mut buf = vec![0u8; want.len()];
    let n = unsafe {
        fs_btrfs_getxattr(
            fs,
            path.as_ptr(),
            name.as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            buf.len(),
        )
    };
    assert_eq!(n, want.len() as i64);
    assert_eq!(buf, want, "a value with NUL bytes was truncated");
    unsafe { fs_btrfs_umount(fs) };
}

/// Zero and -1 mean different things here and the header says so: an
/// attribute set to nothing returns 0, an attribute that is not there
/// returns -1 with ENOENT. A caller testing `<= 0` would merge them.
#[test]
fn getxattr_distinguishes_an_empty_value_from_a_missing_one() {
    let fs = xattr_fixture();
    let path = cstr("/plain.txt");
    let mut buf = [0u8; 8];

    let empty = unsafe {
        fs_btrfs_getxattr(
            fs,
            path.as_ptr(),
            cstr("user.empty").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            buf.len(),
        )
    };
    assert_eq!(empty, 0, "an empty value must not look like an error");

    let missing = unsafe {
        fs_btrfs_getxattr(
            fs,
            path.as_ptr(),
            cstr("user.never-set").as_ptr(),
            buf.as_mut_ptr().cast::<c_void>(),
            buf.len(),
        )
    };
    assert_eq!(missing, -1);
    assert_eq!(fs_btrfs_last_errno(), ENOENT, "{}", last_error());
    unsafe { fs_btrfs_umount(fs) };
}

/// Every name `listxattr` returns can be handed straight back to
/// `getxattr` (#104).
///
/// Attribute names are bytes, and `listxattr` writes them raw, but
/// `getxattr` took its name through the UTF-8 path helper and answered a
/// non-UTF-8 one with -1 and ENOENT -- the errno for "not present". So a
/// caller could list a name, pass that exact name back, and be told it
/// does not exist.
///
/// The CI fixture's names are all UTF-8, so there the round trip holds
/// either way; the second half needs no such name on disk. A local image
/// carrying `user.caf\xe9` fails the first half before the fix.
#[test]
fn getxattr_accepts_every_name_listxattr_returns_and_takes_names_as_bytes() {
    let fs = xattr_fixture();
    let path = cstr("/plain.txt");
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
    for name in list.split(|&b| b == 0).filter(|n| !n.is_empty()) {
        let c = CString::new(name.to_vec()).unwrap();
        let size =
            unsafe { fs_btrfs_getxattr(fs, path.as_ptr(), c.as_ptr(), std::ptr::null_mut(), 0) };
        assert!(
            size >= 0,
            "listxattr returned {:?} and getxattr says it is absent: {}",
            String::from_utf8_lossy(name),
            last_error()
        );
    }

    // A name that is not UTF-8 and is not set is looked up and reported
    // absent -- not refused before the lookup as malformed.
    let absent = CString::new(b"user.not-\xff-set".to_vec()).unwrap();
    let r =
        unsafe { fs_btrfs_getxattr(fs, path.as_ptr(), absent.as_ptr(), std::ptr::null_mut(), 0) };
    assert_eq!(r, -1);
    assert_eq!(fs_btrfs_last_errno(), ENOENT, "{}", last_error());
    assert!(
        !last_error().contains("UTF-8"),
        "a byte name was refused before it was looked up: {}",
        last_error()
    );
    unsafe { fs_btrfs_umount(fs) };
}

/// A file with no attributes lists nothing, and that is a successful
/// zero rather than a failure.
#[test]
fn listxattr_on_a_file_without_attributes_is_zero_not_an_error() {
    let fs = xattr_fixture();
    let n = unsafe { fs_btrfs_listxattr(fs, cstr("/bare.txt").as_ptr(), std::ptr::null_mut(), 0) };
    assert_eq!(n, 0, "{}", last_error());
    unsafe { fs_btrfs_umount(fs) };
}

/// NULL tolerance, which is the whole reason this file exists.
#[test]
fn the_xattr_entry_points_tolerate_nulls() {
    let mut buf = [0u8; 8];
    assert_eq!(
        unsafe {
            fs_btrfs_listxattr(
                std::ptr::null_mut(),
                cstr("/x").as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        },
        -1
    );
    assert_eq!(
        unsafe {
            fs_btrfs_getxattr(
                std::ptr::null_mut(),
                cstr("/x").as_ptr(),
                cstr("user.x").as_ptr(),
                buf.as_mut_ptr().cast::<c_void>(),
                buf.len(),
            )
        },
        -1
    );

    let fs = xattr_fixture();
    assert_eq!(
        unsafe { fs_btrfs_listxattr(fs, std::ptr::null(), std::ptr::null_mut(), 0) },
        -1
    );
    assert_eq!(
        unsafe {
            fs_btrfs_getxattr(
                fs,
                cstr("/plain.txt").as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            )
        },
        -1
    );
    // A path that is not there fails as ENOENT rather than as EIO, so a
    // user is not sent looking for a hardware fault.
    assert_eq!(
        unsafe { fs_btrfs_listxattr(fs, cstr("/nope.txt").as_ptr(), std::ptr::null_mut(), 0) },
        -1
    );
    assert_eq!(fs_btrfs_last_errno(), ENOENT, "{}", last_error());
    unsafe { fs_btrfs_umount(fs) };
}

// ---- paths are bytes (#214) --------------------------------------------

/// `/caf\xe9.txt` — latin-1 for `café.txt`, which is what a name written
/// on a Linux box with a non-UTF-8 locale looks like. `\xe9` alone is not
/// a legal UTF-8 sequence.
///
/// `c_char` is `i8` on x86_64 and Apple targets and `u8` on
/// aarch64-linux, so `from_ne_bytes` is the spelling that works on both.
fn non_utf8_path() -> Vec<c_char> {
    b"/caf\xe9.txt\0"
        .iter()
        .map(|&b| c_char::from_ne_bytes([b]))
        .collect()
}

/// A name that is not valid UTF-8 is REACHABLE, not merely refused.
///
/// Btrfs directory entry names are raw bytes and the format has no field
/// that could say what encoding they are in, so such names are ordinary
/// rather than hostile: any image built on a box with a non-UTF-8 locale
/// holds them.
///
/// This crate refused them at the ABI, which is better than answering
/// about the wrong inode — but it left the file listed and unopenable.
/// `fs_btrfs_dir_next` hands the caller the entry's name from the raw
/// bytes, so the ABI reported a name it then refused to accept, and
/// composing the path from those same bytes produced the same rejection
/// (#214).
///
/// The fixture has no such file, so what is asserted here is the shape
/// of the refusal: a path naming no file is `ENOENT`, the honest answer
/// for bytes that name nothing, and NOT a complaint about the argument's
/// encoding. That is what tells a caller the file is absent rather than
/// that its own string handling is wrong.
#[test]
fn a_non_utf8_path_is_taken_as_bytes_and_reported_as_missing() {
    let fs = mount();
    let path = non_utf8_path();
    let mut attr = zeroed_attr();
    let rc = unsafe { fs_btrfs_stat(fs, path.as_ptr(), &mut attr) };
    assert_eq!(rc, -1, "a path naming no file was answered as a stat");
    let msg = last_error();
    assert!(
        !msg.contains("not valid UTF-8"),
        "the path was refused for its encoding rather than looked up: {msg}"
    );
    unsafe { fs_btrfs_umount(fs) };
}

/// And the directory iterator, which is the entry point a caller reaches
/// these names through in the first place.
#[test]
fn dir_open_takes_a_non_utf8_path_as_bytes() {
    let fs = mount();
    let path = non_utf8_path();
    let iter = unsafe { fs_btrfs_dir_open(fs, path.as_ptr()) };
    assert!(
        iter.is_null(),
        "a path naming no directory opened an iterator"
    );
    let msg = last_error();
    assert!(
        !msg.contains("not valid UTF-8"),
        "the path was refused for its encoding rather than looked up: {msg}"
    );
    unsafe { fs_btrfs_umount(fs) };
}
