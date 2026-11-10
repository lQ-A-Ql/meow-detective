//! Directory entries (`struct btrfs_dir_item`).
//!
//! # Two indexes over one set of names
//!
//! Btrfs files every directory entry twice, under two key types built
//! from the same `btrfs_dir_item` payload:
//!
//! - **`DIR_ITEM`** (`(dir_ino, 84, hash)`) is the lookup index. The key
//!   offset is [`name_hash`] of the entry's name, so resolving a name is
//!   a single keyed descent rather than a scan.
//! - **`DIR_INDEX`** (`(dir_ino, 96, sequence)`) is the listing index.
//!   The key offset is the order the entry was created in, which is what
//!   makes a `readdir` position a stable cookie.
//!
//! Both are needed and neither is redundant: the hash index cannot be
//! enumerated in a meaningful order, and the sequence index cannot be
//! searched by name. This driver reads the first for [`lookup`] and the
//! second for [`read_dir`].
//!
//! [`lookup`]: crate::fs::Filesystem::lookup
//! [`read_dir`]: crate::fs::Filesystem::read_dir
//!
//! # Several entries in one item
//!
//! A `DIR_ITEM` key is a hash, and hashes collide. When two names in one
//! directory hash to the same value they share a key, and their payloads
//! are simply concatenated inside that one item. So the unit of parsing
//! is a *sequence* — [`parse_dir_items`] — not a single entry. Parsing
//! only the first would drop the second name silently, which is exactly
//! the kind of quietly-wrong answer this driver is meant not to give.
//!
//! `XATTR_ITEM` uses the same struct with its `data` field holding the
//! attribute value, and collides in the same way — several attribute
//! names hashing to one key share an item. So the record walk is shared:
//! `parse_items` does the bounds arithmetic once, and
//! [`parse_dir_items`] and [`crate::xattr::parse_xattr_items`] each read
//! the fields they care about out of the result.
//!
//! # Layout
//!
//! ```text
//!    0  location   btrfs_disk_key  (17 bytes, packed)
//!   17  transid    u64
//!   25  data_len   u16
//!   27  name_len   u16
//!   29  type       u8
//!   30  name       name_len bytes
//!       data       data_len bytes
//! ```
//!
//! # Byte order
//!
//! Little-endian throughout, like the rest of the format.
//!
//! # Confidence
//!
//! The 30-byte header is corroborated by real media rather than
//! recalled. Across the `deep4k` and `deep16k` fixtures, all 80,002
//! `DIR_ITEM`s parse with this stride and consume their item data
//! *exactly* — no trailing byte left over on any of them, which a wrong
//! header size or a wrong `name_len` offset could not produce. The
//! decoded names round-trip against the file names the fixtures were
//! built from, and each name's [`name_hash`] reproduces the key offset
//! its item is filed under, which independently confirms both the name
//! span and the hash.
//!
//! The `type` byte's meaning is corroborated for two of its values —
//! `2` on the one directory and `1` on all 80,000 regular files in the
//! deep fixtures. The remaining five are the same compact encoding the
//! sister ext-family and XFS drivers use and are called out at
//! [`ftype_from_raw`].

use crate::chunk::{DiskKey, DISK_KEY_SIZE};
use crate::error::{Error, Result};
use crate::inode::{FileType, INODE_ITEM_KEY};
use crate::superblock::{le16, le64};

/// `BTRFS_DIR_ITEM_KEY` — the name-hash index over a directory.
pub const DIR_ITEM_KEY: u8 = 84;

/// `BTRFS_DIR_INDEX_KEY` — the creation-order index over a directory.
pub const DIR_INDEX_KEY: u8 = 96;

/// `BTRFS_XATTR_ITEM_KEY` — extended attributes, which reuse this
/// struct with the attribute's value in the trailing `data` field. Read
/// by [`crate::xattr`].
pub const XATTR_ITEM_KEY: u8 = 24;

/// Size of the fixed part of a `struct btrfs_dir_item`, before its name.
///
/// The field widths sum to 30: a 17-byte key, a `u64`, two `u16`s and a
/// `u8`. The struct is packed, so `transid` starts on an odd byte.
pub const DIR_ITEM_HEADER_SIZE: usize = DISK_KEY_SIZE + 8 + 2 + 2 + 1;

/// `BTRFS_NAME_LEN` — the longest name a directory entry may hold.
pub const MAX_NAME_LEN: usize = 255;

/// Byte offsets within a `struct btrfs_dir_item`.
pub mod offsets {
    /// `location` — the key of whatever this name resolves to. An
    /// ordinary entry points at an `INODE_ITEM` in the same tree; a
    /// subvolume mount point points at a `ROOT_ITEM` in the root tree.
    pub const LOCATION: usize = 0;
    /// `transid` — the transaction that created the entry.
    pub const TRANSID: usize = 17;
    /// `data_len` — length of the trailing value. Zero for a directory
    /// entry; the attribute value's length for an `XATTR_ITEM`.
    pub const DATA_LEN: usize = 25;
    /// `name_len` — length of the name that follows the header.
    pub const NAME_LEN: usize = 27;
    /// `type` — the entry's file type, one of [`super::ftype`].
    pub const TYPE: usize = 29;
    /// Where the name begins.
    pub const NAME: usize = 30;
}

/// Raw `type` values (`BTRFS_FT_*`).
pub mod ftype {
    /// The filesystem does not record a type for this entry.
    pub const UNKNOWN: u8 = 0;
    /// Regular file.
    pub const REG_FILE: u8 = 1;
    /// Directory.
    pub const DIR: u8 = 2;
    /// Character device.
    pub const CHRDEV: u8 = 3;
    /// Block device.
    pub const BLKDEV: u8 = 4;
    /// FIFO.
    pub const FIFO: u8 = 5;
    /// Unix domain socket.
    pub const SOCK: u8 = 6;
    /// Symbolic link.
    pub const SYMLINK: u8 = 7;
    /// Extended attribute, used only by `XATTR_ITEM`.
    pub const XATTR: u8 = 8;
}

/// Decode a raw `type` byte.
///
/// `Ok(None)` covers [`ftype::UNKNOWN`] — the entry carries no type and
/// the caller must read the inode — and [`ftype::XATTR`], which is not a
/// file type at all. An undefined value is an error rather than a
/// silent `None`, because it means the byte was read from the wrong
/// place.
///
/// # Confidence
///
/// Values `1` and `2` are corroborated against real media. The other
/// five are the compact `BTRFS_FT_*` encoding, which is the same
/// ordering the ext-family and XFS on-disk directories use — see
/// `fs_xfs::dir::ftype_from_raw`, which maps the identical numbers to
/// the identical types. Agreement between two independently written
/// drivers is weaker evidence than a fixture, and is flagged as such.
pub fn ftype_from_raw(raw: u8) -> Result<Option<FileType>> {
    Ok(Some(match raw {
        ftype::UNKNOWN | ftype::XATTR => return Ok(None),
        ftype::REG_FILE => FileType::Regular,
        ftype::DIR => FileType::Directory,
        ftype::CHRDEV => FileType::CharDevice,
        ftype::BLKDEV => FileType::BlockDevice,
        ftype::FIFO => FileType::Fifo,
        ftype::SOCK => FileType::Socket,
        ftype::SYMLINK => FileType::Symlink,
        other => {
            return Err(Error::BadSuperblock(format!(
                "directory entry type {other} is not a defined value"
            )))
        }
    }))
}

/// The seed Btrfs hashes a name with.
///
/// The kernel computes `btrfs_name_hash()` as `crc32c((u32)~1, name,
/// len)`, where its `crc32c()` is a raw CRC continuation with no bit
/// inversion at either end.
const NAME_HASH_SEED: u32 = !1u32;

/// The `DIR_ITEM` key offset a name is filed under.
///
/// # Why this is not simply `crc32c(name)`
///
/// Two things have to line up. The seed is `~1`, not zero. And the
/// `crc32c` crate's [`crc32c_append`](crc32c::crc32c_append) follows the
/// usual CRC convention of complementing the running value on the way in
/// and on the way out, while the kernel's `crc32c()` does neither — so
/// both complements are undone here. Get either wrong and every lookup
/// returns "no such file" for names that plainly exist, which is the
/// worst failure mode this driver has: a confident, wrong answer.
///
/// That is why this function is checked against real media in
/// `tests/fs_oracle.rs`, where every one of the 80,002 `DIR_ITEM` keys
/// in the deep fixtures must equal the hash of its own name.
pub fn name_hash(name: &[u8]) -> u64 {
    u64::from(!crc32c::crc32c_append(!NAME_HASH_SEED, name))
}

/// One directory entry.
///
/// Shaped to match the sister XFS driver's `DirEntry` so a consumer can
/// treat the two drivers alike, with one addition Btrfs forces:
/// [`DirEntry::location_type`]. XFS names only inodes; a Btrfs entry can
/// also name a whole subvolume, and a caller that assumed otherwise
/// would look up a tree's objectid as if it were an inode number and get
/// a confusing "not found" for a directory that is plainly there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The name, exactly as stored. Btrfs does not NUL-terminate names
    /// and does not require them to be valid UTF-8, so they stay as
    /// bytes.
    pub name: Vec<u8>,
    /// Objectid the name resolves to. An inode number when
    /// [`location_type`](Self::location_type) is
    /// [`crate::inode::INODE_ITEM_KEY`]; a subvolume's
    /// tree id otherwise.
    pub ino: u64,
    /// File type, when the entry records one this driver represents.
    /// `None` means the caller must read the inode.
    pub ftype: Option<FileType>,
    /// The `location` key's type: `INODE_ITEM_KEY` for an ordinary
    /// entry, `ROOT_ITEM_KEY` for a subvolume mount point.
    pub location_type: u8,
    /// Transaction that created the entry.
    pub transid: u64,
}

impl DirEntry {
    /// Whether this name resolves to an inode in the same file tree,
    /// rather than to another subvolume's root.
    pub fn is_inode(&self) -> bool {
        self.location_type == INODE_ITEM_KEY
    }
}

/// One record inside a packed `btrfs_dir_item` sequence, before anything
/// has decided what it means.
///
/// Directory entries and extended attributes are the same bytes read two
/// ways: an entry cares about the location key and the type byte and has
/// no value; an attribute cares about the name and the value and has no
/// meaningful location. Splitting the walk from the interpretation keeps
/// one copy of the bounds arithmetic, which is the part that has to be
/// right — a `name_len` or `data_len` mishandled by one byte
/// desynchronises everything after it in the item.
pub(crate) struct RawItem<'a> {
    /// The `location` key. Names an inode or a subvolume root for a
    /// directory entry; unused, and zero in practice, for an attribute.
    pub(crate) location: DiskKey,
    /// Transaction that created the record.
    pub(crate) transid: u64,
    /// The raw `type` byte, undecoded.
    pub(crate) ftype: u8,
    /// The name, exactly as stored.
    pub(crate) name: &'a [u8],
    /// The trailing value. Empty for a directory entry.
    pub(crate) value: &'a [u8],
}

/// Walk the sequence of records packed into one item's data.
///
/// `what` names the caller in every error message, so a malformed
/// extended attribute does not report itself as a malformed directory.
///
/// # Errors
///
/// [`Error::BadSuperblock`] if a header is truncated, a record runs past
/// the end of the data, or a name is empty or longer than
/// [`MAX_NAME_LEN`].
///
/// Those per-record checks are also what catch a wrong stride. Each
/// record is required to fit in what remains, so the walk can only stop
/// exactly at the end, and bytes left over after a misread `name_len` or
/// `data_len` are read as the next record's header: fewer than a header's
/// worth report as truncated, and more usually as an empty, oversized or
/// overrunning name. This used to end with a separate "consumed N of M
/// bytes" check that the loop made unreachable, while the doc called it
/// the load-bearing one (#106).
pub(crate) fn parse_items<'a>(data: &'a [u8], what: &str) -> Result<Vec<RawItem<'a>>> {
    use offsets as o;
    let mut out: Vec<RawItem<'a>> = Vec::new();
    let mut pos = 0usize;
    while pos < data.len() {
        let rest = &data[pos..];
        if rest.len() < DIR_ITEM_HEADER_SIZE {
            return Err(Error::BadSuperblock(format!(
                "{what} {} is truncated: {} bytes left, need {DIR_ITEM_HEADER_SIZE} \
                 for a header",
                out.len(),
                rest.len()
            )));
        }
        let location = DiskKey::parse(&rest[o::LOCATION..])?;
        let data_len = usize::from(le16(rest, o::DATA_LEN));
        let name_len = usize::from(le16(rest, o::NAME_LEN));
        if name_len == 0 {
            return Err(Error::BadSuperblock(format!(
                "{what} {} has an empty name",
                out.len()
            )));
        }
        if name_len > MAX_NAME_LEN {
            return Err(Error::BadSuperblock(format!(
                "{what} {} has a {name_len}-byte name, past the {MAX_NAME_LEN}-byte limit",
                out.len()
            )));
        }
        let name_end = o::NAME + name_len;
        let end = name_end.checked_add(data_len).ok_or_else(|| {
            Error::BadSuperblock(format!("{what} {} has lengths that overflow", out.len()))
        })?;
        if end > rest.len() {
            return Err(Error::BadSuperblock(format!(
                "{what} {} needs {end} bytes but only {} remain",
                out.len(),
                rest.len()
            )));
        }
        out.push(RawItem {
            location,
            transid: le64(rest, o::TRANSID),
            ftype: rest[o::TYPE],
            name: &rest[o::NAME..name_end],
            value: &rest[name_end..end],
        });
        pos += end;
    }
    debug_assert_eq!(pos, data.len(), "every record is checked to fit");
    Ok(out)
}

/// Parse the sequence of dir items packed into one item's data.
///
/// # Errors
///
/// As `parse_items`, plus [`Error::BadSuperblock`] for a `type` byte
/// that is not a defined value.
pub fn parse_dir_items(data: &[u8]) -> Result<Vec<DirEntry>> {
    parse_items(data, "directory item")?
        .into_iter()
        .map(|r| {
            Ok(DirEntry {
                name: r.name.to_vec(),
                ino: r.location.objectid,
                ftype: ftype_from_raw(r.ftype)?,
                location_type: r.location.key_type,
                transid: r.transid,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Unit tests over hand-built directory items.
    //!
    //! **Necessary but not sufficient.** The encoder below uses the same
    //! offsets and byte order the parser decodes with, so a misreading
    //! of `struct btrfs_dir_item` would be baked into both sides and
    //! these tests would pass anyway. What they buy is coverage of the
    //! bounds arithmetic, the packed-sequence handling and the type
    //! decoding. Whether the offsets are right — and whether
    //! [`name_hash`] is the hash Btrfs actually files names under — is
    //! settled by `tests/fs_oracle.rs` against filesystems `mkfs.btrfs`
    //! and the Linux kernel wrote.

    use super::*;

    fn encode(name: &[u8], ino: u64, ft: u8, loc_type: u8, value: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; DIR_ITEM_HEADER_SIZE];
        b[0..8].copy_from_slice(&ino.to_le_bytes());
        b[8] = loc_type;
        b[9..17].copy_from_slice(&0u64.to_le_bytes());
        b[offsets::TRANSID..offsets::TRANSID + 8].copy_from_slice(&42u64.to_le_bytes());
        b[offsets::DATA_LEN..offsets::DATA_LEN + 2]
            .copy_from_slice(&(value.len() as u16).to_le_bytes());
        b[offsets::NAME_LEN..offsets::NAME_LEN + 2]
            .copy_from_slice(&(name.len() as u16).to_le_bytes());
        b[offsets::TYPE] = ft;
        b.extend_from_slice(name);
        b.extend_from_slice(value);
        b
    }

    fn one(name: &[u8], ino: u64, ft: u8) -> Vec<u8> {
        encode(name, ino, ft, INODE_ITEM_KEY, &[])
    }

    #[test]
    fn parses_a_single_entry() {
        let entries = parse_dir_items(&one(b"hello.txt", 257, ftype::REG_FILE)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, b"hello.txt");
        assert_eq!(entries[0].ino, 257);
        assert_eq!(entries[0].ftype, Some(FileType::Regular));
        assert_eq!(entries[0].transid, 42);
        assert!(entries[0].is_inode());
    }

    /// Names that hash-collide share one key and are stored end to end.
    /// Parsing only the first would drop the rest.
    #[test]
    fn parses_several_entries_packed_into_one_item() {
        let mut data = one(b"a", 300, ftype::REG_FILE);
        data.extend(one(b"bb", 301, ftype::DIR));
        data.extend(one(b"ccc", 302, ftype::SYMLINK));
        let entries = parse_dir_items(&data).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, b"a");
        assert_eq!(entries[1].name, b"bb");
        assert_eq!(entries[2].name, b"ccc");
        assert_eq!(entries[1].ftype, Some(FileType::Directory));
        assert_eq!(entries[2].ino, 302);
    }

    /// Bytes left over after the records, however many, are an error:
    /// they are read as another record and fail its checks (#106).
    #[test]
    fn leftover_bytes_after_the_records_are_refused() {
        let good = one(b"hello.txt", 257, ftype::REG_FILE);
        for leftover in 1..=DIR_ITEM_HEADER_SIZE + 12 {
            let mut data = good.clone();
            data.extend(std::iter::repeat_n(0xA5u8, leftover));
            assert!(
                parse_items(&data, "dir item").is_err(),
                "{leftover} leftover bytes were accepted"
            );
            let mut zeros = good.clone();
            zeros.extend(std::iter::repeat_n(0u8, leftover));
            assert!(
                parse_items(&zeros, "dir item").is_err(),
                "{leftover} leftover zero bytes were accepted"
            );
        }
    }

    /// An `XATTR_ITEM` carries a value after the name. The value is
    /// skipped, but skipping it by the wrong amount would desynchronise
    /// everything after it.
    #[test]
    fn a_trailing_value_is_skipped_by_exactly_its_length() {
        let mut data = encode(b"user.thing", 0, ftype::XATTR, 0, b"the value");
        data.extend(one(b"after", 400, ftype::REG_FILE));
        let entries = parse_dir_items(&data).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, b"user.thing");
        assert_eq!(entries[0].ftype, None, "an xattr is not a file type");
        assert_eq!(entries[1].name, b"after");
    }

    #[test]
    fn an_entry_naming_a_subvolume_is_flagged_as_such() {
        // 132 is BTRFS_ROOT_ITEM_KEY.
        let entries = parse_dir_items(&encode(b"sub", 260, ftype::DIR, 132, &[])).unwrap();
        assert!(!entries[0].is_inode());
        assert_eq!(entries[0].location_type, 132);
    }

    #[test]
    fn empty_data_is_an_empty_list() {
        assert!(parse_dir_items(&[]).unwrap().is_empty());
    }

    #[test]
    fn rejects_a_truncated_header() {
        let data = vec![0u8; DIR_ITEM_HEADER_SIZE - 1];
        assert!(matches!(
            parse_dir_items(&data),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_a_name_running_past_the_end() {
        let mut data = one(b"name", 300, ftype::REG_FILE);
        data[offsets::NAME_LEN..offsets::NAME_LEN + 2].copy_from_slice(&200u16.to_le_bytes());
        assert!(matches!(
            parse_dir_items(&data),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_an_empty_name() {
        let mut data = one(b"name", 300, ftype::REG_FILE);
        data[offsets::NAME_LEN..offsets::NAME_LEN + 2].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            parse_dir_items(&data),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_a_name_past_the_format_limit() {
        let mut data = one(b"name", 300, ftype::REG_FILE);
        data[offsets::NAME_LEN..offsets::NAME_LEN + 2]
            .copy_from_slice(&((MAX_NAME_LEN + 1) as u16).to_le_bytes());
        data.resize(DIR_ITEM_HEADER_SIZE + MAX_NAME_LEN + 1, b'x');
        match parse_dir_items(&data) {
            Err(Error::BadSuperblock(m)) => assert!(m.contains("255"), "message: {m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// Bytes left over after the last complete item mean the stride is
    /// wrong, and that has to be an error rather than a silently shorter
    /// listing — a caller cannot tell a truncated directory from a
    /// complete one.
    ///
    /// The remainder is indistinguishable from a genuinely truncated
    /// item at this level, so the message reports it that way, naming
    /// how many bytes are left against how many a header needs. The
    /// assertion checks for those counts rather than for particular
    /// wording, since the useful property is that the message says
    /// enough to diagnose it.
    #[test]
    fn rejects_data_the_entries_do_not_exactly_consume() {
        let mut data = one(b"name", 300, ftype::REG_FILE);
        data.push(0);
        match parse_dir_items(&data) {
            Err(Error::BadSuperblock(m)) => {
                assert!(
                    m.contains('1') && m.contains(&DIR_ITEM_HEADER_SIZE.to_string()),
                    "message should name the leftover byte count and the header size: {m}"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn rejects_an_undefined_type_byte() {
        let data = one(b"name", 300, 99);
        assert!(matches!(
            parse_dir_items(&data),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn decodes_every_defined_type_byte() {
        for (raw, want) in [
            (ftype::REG_FILE, Some(FileType::Regular)),
            (ftype::DIR, Some(FileType::Directory)),
            (ftype::CHRDEV, Some(FileType::CharDevice)),
            (ftype::BLKDEV, Some(FileType::BlockDevice)),
            (ftype::FIFO, Some(FileType::Fifo)),
            (ftype::SOCK, Some(FileType::Socket)),
            (ftype::SYMLINK, Some(FileType::Symlink)),
            (ftype::UNKNOWN, None),
            (ftype::XATTR, None),
        ] {
            assert_eq!(ftype_from_raw(raw).unwrap(), want, "raw type {raw}");
        }
    }

    /// A fixed vector for the hash, computed independently of this
    /// implementation from the definition `crc32c(~1, name)` with no bit
    /// inversion at either end. It pins the two things most likely to be
    /// wrong — the seed and the crate's complementing convention — so a
    /// change to either fails here rather than silently in `lookup`.
    #[test]
    fn name_hash_matches_an_independently_computed_vector() {
        assert_eq!(name_hash(b"hello"), 0x5d9f_2b1f);
        assert_eq!(name_hash(b"f1.txt"), 0xb0c1_9af9);
        assert_eq!(name_hash(b"many"), 0xb870_7cbf);
        assert_eq!(name_hash(b"."), 0xd32c_1e54);
        assert_eq!(name_hash(b".."), 0x1aa9_502b);
        // The empty name never occurs on disk, but it isolates the seed
        // from the CRC itself: with no bytes fed in, the result is the
        // seed unchanged.
        assert_eq!(name_hash(b""), u64::from(NAME_HASH_SEED));
    }

    /// The header size has to be the sum of its fields, and the name has
    /// to start where the header ends.
    #[test]
    fn the_header_arithmetic_closes() {
        assert_eq!(DIR_ITEM_HEADER_SIZE, 30);
        assert_eq!(offsets::NAME, DIR_ITEM_HEADER_SIZE);
        assert_eq!(offsets::TRANSID, DISK_KEY_SIZE);
        assert_eq!(offsets::TYPE + 1, offsets::NAME);
    }
}
