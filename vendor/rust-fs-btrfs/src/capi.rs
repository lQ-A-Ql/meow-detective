//! C ABI (`fs_btrfs_*`), matching `include/fs_btrfs.h`.
//!
//! # Boundary rules
//!
//! Three things must never cross back into C, and each is handled here
//! rather than hoped for:
//!
//! 1. **A panic.** Unwinding into C is undefined behaviour, so every
//!    entry point runs inside [`catch_unwind`] and converts a panic into
//!    the same failure signal any other error produces.
//! 2. **A Rust error type.** Failures become a `-1`/NULL return plus a
//!    thread-local message and errno, which is what a C caller can act
//!    on.
//! 3. **A borrowed pointer.** Handles are boxed and leaked deliberately;
//!    the caller owns one until it calls the matching release function.
//!
//! The error state is thread-local, so two threads failing at once do
//! not overwrite each other's message. Every entry point except the two
//! release functions clears it on entry, so it describes the calling
//! thread's most recent call, never an older failure.
//!
//! # Safety contract for callers
//!
//! Pointers must be either NULL or valid for the type named. A handle
//! must not be used after its release function, nor concurrently from
//! two threads. Every function tolerates NULL by failing rather than
//! dereferencing it.

#![allow(non_camel_case_types)]

use crate::dir::DirEntry;
use crate::error::Error;
use crate::fs::Filesystem;
use crate::inode::{FileType, Inode};
use fs_core::{BlockRead, FileDevice};
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

thread_local! {
    /// Message and errno describing this thread's most recent call: the
    /// failure it reported, or `("no error", 0)` if it succeeded.
    static LAST_ERROR: RefCell<(CString, c_int)> =
        RefCell::new((CString::new("no error").unwrap(), 0));
}

// Spelled out rather than pulled from a crate, to avoid a dependency
// that exists only for a handful of constants.
const ENOENT: c_int = 2;
const EIO: c_int = 5;
const ENOTDIR: c_int = 20;
const EISDIR: c_int = 21;
/// `EINVAL` — a NULL argument, or readlink on something that is not a link.
const EINVAL: c_int = 22;
const EROFS_ERRNO: c_int = 30;
/// `ERANGE` — a result did not fit the caller's buffer.
const ERANGE: c_int = 34;
/// `ENOTSUP` is 45 on Darwin and 95 on Linux.
const fn enotsup() -> c_int {
    if cfg!(target_os = "macos") {
        45
    } else {
        95
    }
}

/// Map a driver error onto the errno a filesystem client expects.
///
/// A client distinguishes "this file is not here" from "this volume is
/// damaged" only by this value. Reporting EIO for a missing file sends a
/// user looking for hardware faults.
fn errno_for(e: &Error) -> c_int {
    match e {
        Error::NotFound => ENOENT,
        Error::NotADirectory => ENOTDIR,
        Error::NotAFile => EISDIR,
        Error::ReadOnly => EROFS_ERRNO,
        // A format the caller asked for that cannot be made.
        Error::InvalidGeometry(_) => EINVAL,
        // A compressed extent, an unsupported profile, or a feature this
        // driver declines are all "the request is valid, this driver
        // cannot serve it" — which is what ENOTSUP means.
        Error::UnsupportedFeature(_)
        | Error::UnsupportedProfile(_)
        | Error::UnsupportedChecksum(_) => enotsup(),
        // Everything else means the volume cannot be trusted.
        Error::NotBtrfs { .. }
        | Error::BadSuperblock(_)
        | Error::BadChunkItem(_)
        | Error::ChecksumMismatch { .. }
        | Error::BlockIdentityMismatch { .. }
        | Error::UnmappedLogical(_)
        | Error::DirtyLog
        | Error::Io(_) => EIO,
    }
}

fn set_error(message: String, errno: c_int) {
    let c = CString::new(message).unwrap_or_else(|_| CString::new("error").unwrap());
    LAST_ERROR.with(|e| *e.borrow_mut() = (c, errno));
}

fn record(e: &Error) {
    set_error(e.to_string(), errno_for(e));
}

/// The state a thread starts in, and the state every successful call
/// leaves behind.
fn clear_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = (CString::new("no error").unwrap(), 0));
}

/// Run an entry point: clear the thread's error, then run `f` under
/// [`release_guard`].
///
/// Clearing first is what makes the error describe *this* call. Without
/// it a failure stays behind after every later success, and the header's
/// promise that a clean end of directory reports errno 0 cannot hold for
/// any thread that has ever failed (#232).
fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    clear_error();
    release_guard(fallback, f)
}

/// Run `f`, converting a panic into a recorded error and `fallback`.
///
/// Used directly only by the release functions, which leave the error
/// state alone: a caller that closes an iterator after a failed
/// `fs_btrfs_dir_next` and only then reads the errno must still find
/// that failure.
///
/// A panic here means a bug in this crate, not a malformed filesystem —
/// parsers return errors for that. It is still caught, because unwinding
/// into C is undefined behaviour and taking the process down is worse
/// than an EIO the caller can report.
fn release_guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            set_error("internal error: the driver panicked".into(), EIO);
            fallback
        }
    }
}

/// Opaque mounted-filesystem handle.
pub struct fs_btrfs_fs {
    fs: Filesystem,
}

/// Opaque directory iterator.
pub struct fs_btrfs_dir_iter {
    entries: Vec<DirEntry>,
    next: usize,
    /// Storage for the entry most recently returned.
    ///
    /// `dir_next` hands back a borrowed pointer rather than filling a
    /// caller-supplied struct, matching the sibling drivers. The pointer
    /// stays valid until the next call on the same iterator, which is
    /// what the header promises.
    current: fs_btrfs_dirent_t,
}

/// Attributes of one filesystem object.
#[repr(C)]
pub struct fs_btrfs_attr_t {
    pub inode: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub nbytes: u64,
    pub atime: i64,
    pub mtime: i64,
    pub ctime: i64,
    pub otime: i64,
    pub link_count: u32,
    pub file_type: u32,
}

/// One directory entry.
#[repr(C)]
pub struct fs_btrfs_dirent_t {
    pub inode: u64,
    pub file_type: u8,
    pub name_len: u8,
    /// Non-zero when the entry names a subvolume rather than an inode.
    /// Btrfs directory entries can point at either, and a caller that
    /// treats a subvolume id as an inode number will look up nonsense.
    pub is_subvolume: u8,
    pub name: [c_char; 256],
}

/// Volume-wide information.
#[repr(C)]
pub struct fs_btrfs_volume_info_t {
    pub sector_size: u32,
    pub node_size: u32,
    pub total_bytes: u64,
    pub bytes_used: u64,
    pub num_devices: u64,
    /// Checksum algorithm: 0 crc32c, 1 xxhash64, 2 sha256, 3 blake2b.
    pub csum_type: u16,
    pub label: [c_char; 256],
    pub fsid: [u8; 16],
    pub metadata_uuid: [u8; 16],
    pub feature_compat: u64,
    pub feature_compat_ro: u64,
    pub feature_incompat: u64,
}

/// Read callback for mounting over a caller-supplied device.
pub type fs_btrfs_read_fn =
    Option<unsafe extern "C" fn(*mut c_void, *mut c_void, u64, u64) -> c_int>;

/// Caller-supplied block device description.
#[repr(C)]
pub struct fs_btrfs_blockdev_cfg_t {
    pub read: fs_btrfs_read_fn,
    pub context: *mut c_void,
    pub size_bytes: u64,
    pub block_size: u32,
}

/// Numeric file type, shared with the header and with the sibling
/// drivers so a consumer can use one mapping for all of them.
fn file_type_code(t: Option<FileType>) -> u32 {
    match t {
        Some(FileType::Regular) => 1,
        Some(FileType::Directory) => 2,
        Some(FileType::CharDevice) => 3,
        Some(FileType::BlockDevice) => 4,
        Some(FileType::Fifo) => 5,
        Some(FileType::Socket) => 6,
        Some(FileType::Symlink) => 7,
        None => 0,
    }
}

/// Message describing the most recent failure on this thread.
///
/// Never NULL, including before any failure.
#[no_mangle]
pub extern "C" fn fs_btrfs_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().0.as_ptr())
}

/// POSIX errno for the most recent failure on this thread.
#[no_mangle]
pub extern "C" fn fs_btrfs_last_errno() -> c_int {
    LAST_ERROR.with(|e| e.borrow().1)
}

/// Borrow a C string, recording an error and returning `None` if it is
/// NULL or not valid UTF-8.
///
/// # Safety
///
/// `p` must be NULL or point to a NUL-terminated string.
/// The bytes of a NUL-terminated in-image path, not decoded.
///
/// EVERY IN-IMAGE NAME COMES THROUGH HERE. Btrfs directory entry names
/// are raw bytes and the format has no field that could say what
/// encoding they are in, so this ABI does not decide: it compares what
/// the caller passed against what the image holds, byte for byte.
///
/// This is what `fs_btrfs_dir_next` already did in the other direction
/// — it fills the entry name from the raw bytes — and the asymmetry was
/// the defect. The ABI reported a name it then refused to accept, so a
/// caller that walked a directory and stat'd each entry failed on
/// exactly the entries this same library had just handed it, with no
/// byte-oriented entry point to work around it (#214).
///
/// Source-compatible for every caller passing UTF-8, because UTF-8 is a
/// byte string too. NULL is still refused, because the argument is the
/// problem.
///
/// # Safety
///
/// `p` must be NULL or point to a NUL-terminated string.
unsafe fn borrow_bytes<'a>(p: *const c_char, what: &str) -> Option<&'a [u8]> {
    if p.is_null() {
        set_error(format!("{what} is NULL"), ENOENT);
        return None;
    }
    Some(unsafe { CStr::from_ptr(p) }.to_bytes())
}

/// A byte path as text, FOR A MESSAGE ONLY.
///
/// `from_utf8_lossy` is exactly wrong for a lookup — it maps distinct
/// names onto one, so two files become indistinguishable — and exactly
/// right for an error string, which a person reads and nothing compares.
/// Never feed the result back into a lookup.
fn shown(path: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(path)
}

unsafe fn borrow_str<'a>(p: *const c_char, what: &str) -> Option<&'a str> {
    if p.is_null() {
        set_error(format!("{what} is NULL"), ENOENT);
        return None;
    }
    match unsafe { CStr::from_ptr(p) }.to_str() {
        Ok(s) => Some(s),
        Err(_) => {
            set_error(format!("{what} is not valid UTF-8"), ENOENT);
            None
        }
    }
}

fn mount_device(device: Arc<dyn BlockRead>) -> *mut fs_btrfs_fs {
    match Filesystem::mount(device) {
        Ok(fs) => Box::into_raw(Box::new(fs_btrfs_fs { fs })),
        Err(e) => {
            record(&e);
            std::ptr::null_mut()
        }
    }
}

/// Mount the image or device at `device_path`.
///
/// # Safety
///
/// `device_path` must be NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_mount(device_path: *const c_char) -> *mut fs_btrfs_fs {
    guard(std::ptr::null_mut(), || {
        let Some(path) = (unsafe { borrow_str(device_path, "device_path") }) else {
            return std::ptr::null_mut();
        };
        match FileDevice::open(path) {
            Ok(dev) => mount_device(Arc::new(dev)),
            Err(e) => {
                // The open failed, so the volume was never inspected.
                // ENOENT rather than EIO: the caller's path is wrong,
                // not the media.
                set_error(format!("opening {path} failed: {e}"), ENOENT);
                std::ptr::null_mut()
            }
        }
    })
}

/// A block device backed by a C read callback.
struct CallbackDevice {
    read: unsafe extern "C" fn(*mut c_void, *mut c_void, u64, u64) -> c_int,
    context: *mut c_void,
    size: u64,
}

// The caller promises the callback and its context are usable from the
// thread that owns the handle. The header documents handles as
// single-threaded, so this is the same contract stated there.
unsafe impl Send for CallbackDevice {}
unsafe impl Sync for CallbackDevice {}

impl BlockRead for CallbackDevice {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> fs_core::Result<()> {
        let rc = unsafe {
            (self.read)(
                self.context,
                buf.as_mut_ptr().cast::<c_void>(),
                offset,
                buf.len() as u64,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(fs_core::Error::Io(std::io::Error::other(format!(
                "the caller's read callback returned {rc} for {} bytes at offset {offset}",
                buf.len()
            ))))
        }
    }

    fn size_bytes(&self) -> u64 {
        self.size
    }
}

/// Mount over a caller-supplied reader.
///
/// # Safety
///
/// `cfg` must be NULL or point to a valid configuration whose `read`
/// callback is safe to call with the given context.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_mount_with_callbacks(
    cfg: *const fs_btrfs_blockdev_cfg_t,
) -> *mut fs_btrfs_fs {
    guard(std::ptr::null_mut(), || {
        if cfg.is_null() {
            set_error("cfg is NULL".into(), EIO);
            return std::ptr::null_mut();
        }
        let cfg = unsafe { &*cfg };
        let Some(read) = cfg.read else {
            set_error("cfg.read is NULL".into(), EIO);
            return std::ptr::null_mut();
        };
        mount_device(Arc::new(CallbackDevice {
            read,
            context: cfg.context,
            size: cfg.size_bytes,
        }))
    })
}

/// Mount over an existing `fs_core` device handle.
///
/// This is how the FSKit extension mounts a *partition* rather than a
/// whole disk: the host wraps the block-device resource as an
/// `FsCoreDevice`, slices the partition out of it, and hands the slice
/// here. Without it a caller could only ever mount from offset zero.
///
/// # Safety
///
/// `handle` must be NULL or a live `FsCoreDevice` from `fs_core`.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_mount_with_fs_core_device(
    handle: *mut fs_core::ffi::FsCoreDevice,
) -> *mut fs_btrfs_fs {
    guard(std::ptr::null_mut(), || {
        if handle.is_null() {
            set_error("fs_core handle is NULL".into(), EIO);
            return std::ptr::null_mut();
        }
        // This driver is read-only, so only the read half of the device
        // trait is needed.
        let dev: std::sync::Arc<dyn fs_core::BlockDevice> = unsafe { (*handle).inner().clone() };
        let read: std::sync::Arc<dyn BlockRead> = dev;
        mount_device(read)
    })
}

/// Release a mounted-filesystem handle. Safe to call with NULL.
///
/// # Safety
///
/// `fs` must be NULL or a handle from a successful mount that has not
/// already been released.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_umount(fs: *mut fs_btrfs_fs) {
    if fs.is_null() {
        return;
    }
    release_guard((), || drop(unsafe { Box::from_raw(fs) }));
}

/// # Safety
///
/// `fs` must be a live handle; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_get_volume_info(
    fs: *mut fs_btrfs_fs,
    out: *mut fs_btrfs_volume_info_t,
) -> c_int {
    guard(-1, || {
        if fs.is_null() || out.is_null() {
            set_error("fs or out is NULL".into(), EIO);
            return -1;
        }
        let sb = unsafe { &*fs }.fs.superblock();

        let mut label = [0 as c_char; 256];
        for (slot, b) in label.iter_mut().zip(sb.label.as_bytes()).take(255) {
            *slot = *b as c_char;
        }

        unsafe {
            *out = fs_btrfs_volume_info_t {
                sector_size: sb.sectorsize,
                node_size: sb.nodesize,
                total_bytes: sb.total_bytes,
                bytes_used: sb.bytes_used,
                num_devices: sb.num_devices,
                csum_type: sb.csum_type.to_raw(),
                label,
                fsid: sb.fsid,
                metadata_uuid: sb.metadata_uuid,
                feature_compat: sb.compat_flags,
                feature_compat_ro: sb.compat_ro_flags,
                feature_incompat: sb.incompat_flags,
            };
        }
        0
    })
}

fn fill_attr(inode: &Inode, out: *mut fs_btrfs_attr_t) {
    unsafe {
        *out = fs_btrfs_attr_t {
            inode: inode.ino,
            mode: inode.mode,
            uid: inode.uid,
            gid: inode.gid,
            size: inode.size,
            nbytes: inode.nbytes,
            atime: inode.atime.sec,
            mtime: inode.mtime.sec,
            ctime: inode.ctime.sec,
            otime: inode.otime.sec,
            link_count: inode.nlink,
            file_type: file_type_code(inode.file_type()),
        };
    }
}

/// Attributes of `path`. Symbolic links are NOT followed.
///
/// # Safety
///
/// `fs` must be live; `path` NUL-terminated; `out` writable.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_stat(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
    out: *mut fs_btrfs_attr_t,
) -> c_int {
    guard(-1, || {
        if fs.is_null() || out.is_null() {
            set_error("fs or out is NULL".into(), EIO);
            return -1;
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return -1;
        };
        match unsafe { &*fs }.fs.lookup_path_bytes(path) {
            Ok(inode) => {
                fill_attr(&inode, out);
                0
            }
            Err(e) => {
                record(&e);
                -1
            }
        }
    })
}

/// Attributes of an inode by number.
///
/// # Safety
///
/// `fs` must be live; `out` writable.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_stat_ino(
    fs: *mut fs_btrfs_fs,
    inode: u64,
    out: *mut fs_btrfs_attr_t,
) -> c_int {
    guard(-1, || {
        if fs.is_null() || out.is_null() {
            set_error("fs or out is NULL".into(), EIO);
            return -1;
        }
        match unsafe { &*fs }.fs.read_inode(inode) {
            Ok(i) => {
                fill_attr(&i, out);
                0
            }
            Err(e) => {
                record(&e);
                -1
            }
        }
    })
}

/// Open a directory for iteration.
///
/// The whole listing is materialised up front. A streaming iterator
/// would need to hold a borrow of the filesystem across the C boundary,
/// which is a lifetime this ABI cannot express safely.
///
/// # Safety
///
/// `fs` must be live; `path` NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_dir_open(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
) -> *mut fs_btrfs_dir_iter {
    guard(std::ptr::null_mut(), || {
        if fs.is_null() {
            set_error("fs is NULL".into(), EIO);
            return std::ptr::null_mut();
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return std::ptr::null_mut();
        };
        match unsafe { &*fs }.fs.list_path_bytes(path) {
            Ok(entries) => Box::into_raw(Box::new(fs_btrfs_dir_iter {
                entries,
                next: 0,
                current: unsafe { std::mem::zeroed() },
            })),
            Err(e) => {
                record(&e);
                std::ptr::null_mut()
            }
        }
    })
}

/// Next entry: 1 when `out` was filled, 0 at end, -1 on failure.
///
/// # Safety
///
/// `iter` must be live; `out` writable.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_dir_next(
    iter: *mut fs_btrfs_dir_iter,
) -> *const fs_btrfs_dirent_t {
    guard(std::ptr::null(), || {
        if iter.is_null() {
            set_error("iter is NULL".into(), EIO);
            return std::ptr::null();
        }
        let it = unsafe { &mut *iter };
        let Some(e) = it.entries.get(it.next) else {
            return std::ptr::null();
        };
        it.next += 1;

        // The name field is fixed at 256 bytes and must stay
        // NUL-terminated, so a longer name is truncated rather than
        // overrunning. Btrfs caps names at 255 bytes, so this only
        // trims the terminator's worth in the pathological case.
        let mut name = [0 as c_char; 256];
        let n = e.name.len().min(255);
        for (slot, b) in name.iter_mut().zip(&e.name[..n]) {
            *slot = *b as c_char;
        }
        it.current = fs_btrfs_dirent_t {
            inode: e.ino,
            file_type: file_type_code(e.ftype) as u8,
            name_len: n as u8,
            is_subvolume: u8::from(!e.is_inode()),
            name,
        };
        &it.current
    })
}

/// Release an iterator. Safe to call with NULL.
///
/// # Safety
///
/// `iter` must be NULL or a live iterator not already released.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_dir_close(iter: *mut fs_btrfs_dir_iter) {
    if iter.is_null() {
        return;
    }
    release_guard((), || drop(unsafe { Box::from_raw(iter) }));
}

/// Read up to `length` bytes of `path` from `offset`.
///
/// Returns bytes read, 0 at end of file, or -1 on failure. Holes and
/// preallocated extents read as zeros. A compressed extent is decoded
/// (zlib, LZO or zstd) and its decoded bytes returned; one that does not
/// decode fails rather than returning compressed bytes a caller could not
/// tell from a corrupt file.
///
/// # Safety
///
/// `fs` must be live; `path` NUL-terminated; `buf` writable for
/// `length` bytes.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_read_file(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
    buf: *mut c_void,
    offset: u64,
    length: u64,
) -> i64 {
    guard(-1, || {
        if fs.is_null() || buf.is_null() {
            set_error("fs or buf is NULL".into(), EIO);
            return -1;
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return -1;
        };
        let fs = &unsafe { &*fs }.fs;
        let found = match fs.lookup_path_bytes(path) {
            Ok(i) => i,
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        let out = unsafe { std::slice::from_raw_parts_mut(buf.cast::<u8>(), length as usize) };
        match fs.read_at(found.ino, offset, out) {
            Ok(n) => n as i64,
            Err(e) => {
                record(&e);
                -1
            }
        }
    })
}

/// Target of a symbolic link: its length on success, or -1.
///
/// The family's readlink contract, shared by every driver so a layer
/// above needs one shape for it:
///
/// - success returns the target length in bytes EXCLUDING the NUL, as
///   Linux `readlink(2)` does, and writes the target followed by a NUL;
/// - `bufsize < length + 1` returns -1 with `ERANGE`, a message naming the
///   size needed, and NOTHING written into `buf` — never a truncation;
/// - a NULL `fs`, `path` or `buf` returns -1 with `EINVAL`;
/// - a path that is not a symlink returns -1 with `EINVAL`, as
///   `readlink(2)` does, and any other failure -1 with this call's errno.
///
/// # Safety
///
/// `fs` must be live; `path` NUL-terminated; `buf` writable for
/// `bufsize` bytes.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_readlink(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
    buf: *mut c_char,
    bufsize: usize,
) -> c_int {
    guard(-1, || {
        if fs.is_null() || path.is_null() || buf.is_null() {
            set_error("readlink: fs, path or buf is NULL".into(), EINVAL);
            return -1;
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return -1;
        };
        let fs = &unsafe { &*fs }.fs;
        let found = match fs.lookup_path_bytes(path) {
            Ok(i) => i,
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        if !found.is_symlink() {
            set_error(
                format!("readlink: {} is not a symbolic link", shown(path)),
                EINVAL,
            );
            return -1;
        }
        match fs.read_link(found.ino) {
            Ok(target) => {
                // Refuse rather than truncate. A truncated symlink target
                // is a path to somewhere else, and a caller following it
                // has no way to tell — so a buffer that cannot hold the
                // whole target plus its terminator is an error, not a
                // partial success, and nothing is written. ERANGE tells
                // the caller to retry with a larger buffer.
                let needed = target.len() + 1;
                if needed > bufsize {
                    set_error(
                        format!(
                            "readlink buffer holds {bufsize} bytes, need {needed} for the target \
                             and its terminator"
                        ),
                        ERANGE,
                    );
                    return -1;
                }
                // `read_link` bounds a target by PATH_MAX, so the length
                // always fits the return type; refuse rather than wrap if
                // that ever stops being true.
                let Ok(len) = c_int::try_from(target.len()) else {
                    set_error(
                        format!("readlink target of {} bytes is too long", target.len()),
                        EIO,
                    );
                    return -1;
                };
                let out = unsafe { std::slice::from_raw_parts_mut(buf.cast::<u8>(), needed) };
                out[..target.len()].copy_from_slice(&target);
                out[target.len()] = 0;
                len
            }
            Err(e) => {
                record(&e);
                -1
            }
        }
    })
}

// ---------------------------------------------------------------------
// Extended attributes (read only)
//
// The signatures match `fs_ext4_listxattr` / `fs_ext4_getxattr` byte for
// byte, deliberately: a layer above should not need a per-driver shape
// for something every filesystem in the family has. What differs is
// underneath — Btrfs stores the namespace prefix as part of the name, so
// there is no prefix table to expand on the way out.
// ---------------------------------------------------------------------

/// NUL-separated attribute names for `path`, or -1.
///
/// Returns the total size the names need, whether or not they were
/// written, so a caller can probe with a NULL buffer and then allocate.
/// A buffer too small takes as many whole names as fit — a half-written
/// name is not a name, and the caller learns the real size from the
/// return value either way.
///
/// # Safety
///
/// `fs` must be live; `path` NUL-terminated; `buf` writable for
/// `bufsize` bytes, or NULL.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_listxattr(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
    buf: *mut c_char,
    bufsize: usize,
) -> i64 {
    guard(-1, || {
        if fs.is_null() {
            set_error("fs is NULL".into(), EIO);
            return -1;
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return -1;
        };
        let fs = &unsafe { &*fs }.fs;
        let found = match fs.lookup_path_bytes(path) {
            Ok(i) => i,
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        let entries = match fs.list_xattrs(found.ino) {
            Ok(v) => v,
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        let required: usize = entries.iter().map(|e| e.name.len() + 1).sum();
        if !buf.is_null() && bufsize > 0 {
            let out = unsafe { std::slice::from_raw_parts_mut(buf.cast::<u8>(), bufsize) };
            let mut pos = 0usize;
            for e in &entries {
                let needed = e.name.len() + 1;
                if pos + needed > bufsize {
                    break;
                }
                out[pos..pos + e.name.len()].copy_from_slice(&e.name);
                out[pos + e.name.len()] = 0;
                pos += needed;
            }
        }
        required as i64
    })
}

/// One attribute's value for `path`, or -1 when it is not set.
///
/// A zero-length value returns 0 and is not an error; the absent case is
/// -1 with ENOENT. A caller that tested for `<= 0` would conflate them,
/// which is why the header says so too.
///
/// # Safety
///
/// `fs` must be live; `path` and `name` NUL-terminated; `buf` writable
/// for `bufsize` bytes, or NULL.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_getxattr(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
    name: *const c_char,
    buf: *mut c_void,
    bufsize: usize,
) -> i64 {
    guard(-1, || {
        if fs.is_null() {
            set_error("fs is NULL".into(), EIO);
            return -1;
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return -1;
        };
        // A NAME IS BYTES. `listxattr` hands names out raw and Linux
        // allows any NUL-terminated bytes, so the UTF-8 path helper -- which
        // answers a non-UTF-8 name with ENOENT, "not present" -- made a
        // listed name impossible to read back (#104).
        if name.is_null() {
            set_error("name is NULL".into(), ENOENT);
            return -1;
        }
        let name = unsafe { CStr::from_ptr(name) }.to_bytes();
        let fs = &unsafe { &*fs }.fs;
        let found = match fs.lookup_path_bytes(path) {
            Ok(i) => i,
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        let value = match fs.get_xattr(found.ino, name) {
            Ok(Some(v)) => v,
            Ok(None) => {
                set_error(
                    format!(
                        "{} has no attribute {}",
                        shown(path),
                        String::from_utf8_lossy(name)
                    ),
                    ENOENT,
                );
                return -1;
            }
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        if !buf.is_null() && bufsize > 0 {
            let take = value.len().min(bufsize);
            let out = unsafe { std::slice::from_raw_parts_mut(buf.cast::<u8>(), bufsize) };
            out[..take].copy_from_slice(&value[..take]);
        }
        value.len() as i64
    })
}

// ---------------------------------------------------------------------
// Writing
//
// Btrfs is copy-on-write, so almost nothing can be written in place. The
// exception is a file marked NODATACOW — `chattr +C` — whose blocks are
// overwritten where they lie and carry no checksums. Those, and only
// those, can be written without a transaction engine.
//
// Everything else is refused with ENOTSUP by name, so a caller can tell
// "this filesystem cannot do that yet" from "you passed something wrong".
// ---------------------------------------------------------------------

/// Mount the image or device at `device_path` for reading **and
/// writing**.
///
/// Returns NULL if the device cannot be written, if the volume's log
/// tree is non-empty, or for any reason [`fs_btrfs_mount`] would.
///
/// # Safety
///
/// `device_path` must be NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_mount_rw(device_path: *const c_char) -> *mut fs_btrfs_fs {
    guard(std::ptr::null_mut(), || {
        let Some(path) = (unsafe { borrow_str(device_path, "device_path") }) else {
            return std::ptr::null_mut();
        };
        match FileDevice::open_rw(path) {
            Ok(dev) => match Filesystem::mount_rw(Arc::new(dev)) {
                Ok(fs) => Box::into_raw(Box::new(fs_btrfs_fs { fs })),
                Err(e) => {
                    record(&e);
                    std::ptr::null_mut()
                }
            },
            Err(e) => {
                set_error(format!("opening {path} for writing failed: {e}"), EIO);
                std::ptr::null_mut()
            }
        }
    })
}

/// Whether this handle can write.
///
/// Lets a caller ask rather than discover: presenting a volume as
/// writable and then failing every write is worse than knowing up front.
///
/// # Safety
///
/// `fs` must be a live handle or NULL.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_is_writable(fs: *mut fs_btrfs_fs) -> c_int {
    guard(0, || {
        if fs.is_null() {
            return 0;
        }
        c_int::from(unsafe { &*fs }.fs.is_writable())
    })
}

/// Whether `path` can be written in place.
///
/// Answers the question a caller actually has — "will a write to this
/// file succeed?" — without making them attempt one and interpret the
/// failure. A file qualifies only if it is NODATACOW, unchecksummed,
/// and its extents are unshared, uncompressed and really allocated.
///
/// Returns 1 for yes, 0 for no, −1 if the file could not be examined.
///
/// # Safety
///
/// `fs` must be a live handle and `path` NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_can_write_in_place(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
) -> c_int {
    guard(-1, || {
        if fs.is_null() {
            set_error("fs is NULL".into(), EIO);
            return -1;
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return -1;
        };
        let fs = &unsafe { &*fs }.fs;
        let found = match fs.lookup_path_bytes(path) {
            Ok(i) => i,
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        match fs.can_write_in_place(found.ino) {
            Ok(yes) => c_int::from(yes),
            Err(e) => {
                record(&e);
                -1
            }
        }
    })
}

/// Overwrite `length` bytes of an existing file at `offset`.
///
/// Returns the number of bytes written, or −1 with the error recorded.
/// The whole range is written or none of it is.
///
/// Only a NODATACOW file can be written, and only where its extents are
/// unshared, uncompressed and really allocated. Everything else — an
/// ordinary copy-on-write file, a snapshotted extent, a compressed or
/// inline one, a hole, or a write past the end — is refused with
/// ENOTSUP, because each needs a transaction this driver cannot make.
///
/// # Safety
///
/// `fs` must be a live handle; `path` NUL-terminated; `buf` readable for
/// `length` bytes.
#[no_mangle]
pub unsafe extern "C" fn fs_btrfs_write_file(
    fs: *mut fs_btrfs_fs,
    path: *const c_char,
    buf: *const c_void,
    offset: u64,
    length: u64,
) -> i64 {
    guard(-1, || {
        if fs.is_null() || buf.is_null() {
            set_error("fs or buf is NULL".into(), EIO);
            return -1;
        }
        let Some(path) = (unsafe { borrow_bytes(path, "path") }) else {
            return -1;
        };
        let fs = &unsafe { &*fs }.fs;
        let found = match fs.lookup_path_bytes(path) {
            Ok(i) => i,
            Err(e) => {
                record(&e);
                return -1;
            }
        };
        let data = unsafe { std::slice::from_raw_parts(buf.cast::<u8>(), length as usize) };
        match fs.write_at(found.ino, offset, data) {
            Ok(n) => n as i64,
            Err(e) => {
                record(&e);
                -1
            }
        }
    })
}
