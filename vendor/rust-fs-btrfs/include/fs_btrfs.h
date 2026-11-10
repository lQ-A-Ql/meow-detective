/*
 * fs_btrfs.h — C ABI for the pure-Rust Btrfs driver.
 *
 * Link against libfs_btrfs.a. Distinct handles are independent; a single
 * handle must not be used concurrently from two threads.
 *
 * Error convention: functions returning int return 0 on success and -1
 * on failure; functions returning a pointer return NULL. In either case
 * fs_btrfs_last_error() gives a message for the calling thread and
 * fs_btrfs_last_errno() a POSIX errno suitable for returning to a
 * filesystem client. Every call except fs_btrfs_umount and
 * fs_btrfs_dir_close clears both on entry, so after a success the errno
 * is 0 and the message "no error"; they describe the thread's most
 * recent call, not its most recent failure.
 *
 * The driver reads, and writes within narrow limits: fs_btrfs_mount_rw
 * opens a volume for writing and fs_btrfs_write_file overwrites a
 * nodatacow file in place; everything else is refused. It refuses rather
 * than guesses, when reading as well: a compressed
 * extent fails with ENOTSUP rather than returning its undecoded bytes,
 * because a caller cannot distinguish those from a corrupt file.
 */

#ifndef FS_BTRFS_H
#define FS_BTRFS_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

struct FsCoreDevice;

typedef struct fs_btrfs_fs fs_btrfs_fs_t;
typedef struct fs_btrfs_dir_iter fs_btrfs_dir_iter_t;

/* File types, matching the values used by the sibling drivers. */
typedef enum {
    FS_BTRFS_FT_UNKNOWN = 0,
    FS_BTRFS_FT_REGULAR = 1,
    FS_BTRFS_FT_DIRECTORY = 2,
    FS_BTRFS_FT_CHARDEV = 3,
    FS_BTRFS_FT_BLOCKDEV = 4,
    FS_BTRFS_FT_FIFO = 5,
    FS_BTRFS_FT_SOCKET = 6,
    FS_BTRFS_FT_SYMLINK = 7
} fs_btrfs_file_type_t;

typedef struct {
    uint64_t inode;
    uint32_t mode;        /* permission bits and type, as on disk */
    uint32_t uid;
    uint32_t gid;
    uint64_t size;
    uint64_t nbytes;      /* bytes actually allocated */
    int64_t  atime;       /* unix epoch seconds */
    int64_t  mtime;
    int64_t  ctime;
    int64_t  otime;       /* creation time */
    uint32_t link_count;
    uint32_t file_type;   /* fs_btrfs_file_type_t */
} fs_btrfs_attr_t;

typedef struct {
    uint64_t inode;
    uint8_t  file_type;   /* fs_btrfs_file_type_t */
    uint8_t  name_len;
    /*
     * Non-zero when this entry names a SUBVOLUME rather than an inode.
     * Btrfs directory entries may point at either, and `inode` then
     * holds a tree id. A caller that treats it as an inode number will
     * look up nonsense, so check this before using it.
     */
    uint8_t  is_subvolume;
    char     name[256];   /* NUL-terminated */
} fs_btrfs_dirent_t;

typedef struct {
    uint32_t sector_size;
    uint32_t node_size;
    uint64_t total_bytes;
    uint64_t bytes_used;
    uint64_t num_devices;
    uint16_t csum_type;   /* 0 crc32c, 1 xxhash64, 2 sha256, 3 blake2b */
    char     label[256];  /* NUL-terminated */
    uint8_t  fsid[16];
    uint8_t  metadata_uuid[16];
    uint64_t feature_compat;
    uint64_t feature_compat_ro;
    uint64_t feature_incompat;
} fs_btrfs_volume_info_t;

/*
 * Read callback. Must fill exactly `length` bytes at `offset` and return
 * 0, or return non-zero. A short read is a failure, not a partial
 * success.
 */
typedef int (*fs_btrfs_read_fn)(void *context, void *buf,
                                uint64_t offset, uint64_t length);

typedef struct {
    fs_btrfs_read_fn read;
    void            *context;
    uint64_t         size_bytes;
    uint32_t         block_size;
} fs_btrfs_blockdev_cfg_t;

/* ---- diagnostics ---- */

/* Never NULL, including before any failure. */
const char *fs_btrfs_last_error(void);
int fs_btrfs_last_errno(void);

/* ---- mounting ---- */

fs_btrfs_fs_t *fs_btrfs_mount(const char *device_path);
fs_btrfs_fs_t *fs_btrfs_mount_with_callbacks(const fs_btrfs_blockdev_cfg_t *cfg);
/*
 * Mount over an existing fs_core device handle. This is how a caller
 * mounts a PARTITION rather than a whole disk: wrap the block device
 * as an FsCoreDevice, slice the partition out, and pass the slice.
 */
fs_btrfs_fs_t *fs_btrfs_mount_with_fs_core_device(struct FsCoreDevice *handle);

void fs_btrfs_umount(fs_btrfs_fs_t *fs);
int fs_btrfs_get_volume_info(fs_btrfs_fs_t *fs, fs_btrfs_volume_info_t *out);

/* ---- lookup and metadata ---- */

/* Symbolic links are NOT followed; describes the link itself. */
int fs_btrfs_stat(fs_btrfs_fs_t *fs, const char *path, fs_btrfs_attr_t *out);
int fs_btrfs_stat_ino(fs_btrfs_fs_t *fs, uint64_t inode, fs_btrfs_attr_t *out);

/* ---- directories ---- */

fs_btrfs_dir_iter_t *fs_btrfs_dir_open(fs_btrfs_fs_t *fs, const char *path);
/*
 * Next entry, or NULL at end of directory (and on failure — check
 * fs_btrfs_last_errno to tell them apart; it is 0 at a clean end).
 * Owned by the iterator; valid only until the next call on it.
 */
const fs_btrfs_dirent_t *fs_btrfs_dir_next(fs_btrfs_dir_iter_t *iter);
void fs_btrfs_dir_close(fs_btrfs_dir_iter_t *iter);

/* ---- file contents ---- */

/*
 * Bytes read, 0 at end of file, or -1. Holes and preallocated extents
 * read as zeros; a preallocated extent's blocks hold whatever previously
 * occupied them, so returning their contents would disclose it.
 */
int64_t fs_btrfs_read_file(fs_btrfs_fs_t *fs, const char *path,
                           void *buf, uint64_t offset, uint64_t length);

/*
 * Target of the symbolic link at `path`. The contract every driver in
 * the family shares:
 *
 *   success     returns the target length in bytes, EXCLUDING the NUL
 *               (the value Linux readlink(2) returns), and writes the
 *               target followed by a NUL terminator into buf. Test for
 *               success with `>= 0`, not `== 0`.
 *   too small   bufsize < length + 1 returns -1, fs_btrfs_last_errno()
 *               is ERANGE, fs_btrfs_last_error() names the size needed,
 *               and NOTHING is written into buf. Never a truncation —
 *               deliberately unlike Linux: a truncated target is a path
 *               to somewhere else, and a caller following it could not
 *               tell.
 *   NULL        a NULL fs, path or buf returns -1 with EINVAL.
 *   not a link  returns -1 with EINVAL, as readlink(2) does.
 *   otherwise   -1, with fs_btrfs_last_errno() set by this call (ENOENT
 *               for a missing path, EIO for a damaged volume, ...).
 */
int fs_btrfs_readlink(fs_btrfs_fs_t *fs, const char *path,
                      char *buf, size_t bufsize);

/* ---- extended attributes (read only) ---- */

/*
 * List extended attribute names for a path.
 *
 * Writes NUL-separated fully-qualified names ("user.colour\0user.tag\0")
 * into buf. If buf is NULL or bufsize is 0, no bytes are written but the
 * return value still reports the required total size -- use this to probe.
 *
 * Btrfs stores the namespace prefix as part of the name, so what comes
 * back is exactly what setfattr was given. There is no prefix table to
 * expand, unlike ext4 and EROFS.
 *
 * Returns: total bytes of output (names + NUL terminators) on success,
 *          -1 on error. If bufsize is less than the required size, writes
 *          as many WHOLE names as fit and still returns the required size.
 *
 * The signature, the probe, the size returned and the whole-names-only
 * truncation match fs_ext4_listxattr's implementation, so a layer above
 * can treat the two alike for a successful call. (fs_ext4.h says ext4
 * writes "as much as fits"; its code, like this, stops at the last whole
 * name.) The errors differ: here a NULL fs is EIO and a NULL or non-UTF-8
 * path is ENOENT, where fs_ext4_listxattr reports EINVAL for a NULL fs or
 * path.
 */
int64_t fs_btrfs_listxattr(fs_btrfs_fs_t *fs, const char *path,
                           char *buf, size_t bufsize);

/*
 * Get one extended attribute value by fully-qualified name
 * (e.g. "user.colour").
 *
 * Writes raw value bytes (no NUL terminator) into buf. If buf is NULL or
 * bufsize is 0, returns the value size without writing -- use this to probe.
 *
 * A zero-length value is a real value and returns 0, which is NOT an
 * error; an absent attribute returns -1 with ENOENT. Check the return
 * against -1, not against 0.
 *
 * Returns: value size in bytes on success,
 *          -1 if the name is not present or on error. If bufsize is less
 *          than the value size, writes as much as fits and still returns
 *          the value size.
 */
int64_t fs_btrfs_getxattr(fs_btrfs_fs_t *fs, const char *path,
                          const char *name, void *buf, size_t bufsize);

/* ---- writing ---- */

/*
 * Btrfs is copy-on-write: a change normally allocates a new block,
 * records a new checksum, rewrites the B-tree path to the root and
 * commits a new generation. None of that is possible without a
 * transaction engine, so almost nothing here can be written in place.
 *
 * The exception is a file marked NODATACOW -- `chattr +C` -- whose
 * blocks are overwritten where they lie and carry no checksums. Those,
 * and only those, can be written.
 *
 * NODATACOW is not on its own enough. It promises in-place writes only
 * while the extent belongs to one file; a snapshot makes it shared, and
 * the next write must copy first so the snapshot keeps seeing what it
 * saw. Sharing is checked before any write.
 *
 * Everything outside that is refused with ENOTSUP, so a caller can tell
 * "this is not supported yet" from "you passed something wrong".
 */

/*
 * Mount for reading and writing. Returns NULL if the device cannot be
 * written, if the volume's log tree is non-empty, or for any reason
 * fs_btrfs_mount would.
 */
fs_btrfs_fs_t *fs_btrfs_mount_rw(const char *device_path);

/* Whether this handle can write. Ask rather than discover. */
int fs_btrfs_is_writable(fs_btrfs_fs_t *fs);

/*
 * Whether `path` can be written in place: NODATACOW, unchecksummed, and
 * with every extent unshared, uncompressed and really allocated.
 *
 * Returns 1 for yes, 0 for no, -1 if the file could not be examined.
 * Lets a caller decide what to offer without attempting a write and
 * interpreting the failure.
 */
int fs_btrfs_can_write_in_place(fs_btrfs_fs_t *fs, const char *path);

/*
 * Overwrite `length` bytes of an existing file at `offset`. Returns the
 * number written, or -1. The whole range is written or none of it is.
 *
 * Refused with ENOTSUP for an ordinary copy-on-write file, a snapshotted
 * extent, a compressed or inline one, a hole, or a write past the end.
 */
int64_t fs_btrfs_write_file(fs_btrfs_fs_t *fs, const char *path,
                            const void *buf, uint64_t offset, uint64_t length);

#ifdef __cplusplus
}
#endif

#endif /* FS_BTRFS_H */
