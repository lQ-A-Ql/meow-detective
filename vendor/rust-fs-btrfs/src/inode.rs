//! On-disk inode (`struct btrfs_inode_item`) parsing.
//!
//! # Where an inode lives
//!
//! Btrfs has no inode table. An inode is an *item* in a subvolume's file
//! tree, filed under the key `(ino, INODE_ITEM_KEY, 0)`, and its number
//! is the key's objectid rather than anything stored in the item. That
//! is why [`Inode::parse`] is handed the number instead of reading one:
//! there is nothing in these 160 bytes that says which inode they
//! describe.
//!
//! It also means the identity checks the sister XFS driver performs on
//! every inode — "does this record agree that it is inode N?" — have no
//! counterpart here. What stands in their place is one level up: the
//! tree block the item came from carries a checksum, its own logical
//! address and the filesystem UUID, all three of which
//! [`crate::btree::TreeBlock::parse`] verifies before any item inside it
//! is handed out.
//!
//! # Layout
//!
//! ```text
//!    0  generation          u64
//!    8  transid             u64
//!   16  size                u64
//!   24  nbytes              u64
//!   32  block_group         u64
//!   40  nlink               u32
//!   44  uid                 u32
//!   48  gid                 u32
//!   52  mode                u32
//!   56  rdev                u64
//!   64  flags               u64
//!   72  sequence            u64
//!   80  reserved[4]         u64 x 4
//!  112  atime               btrfs_timespec
//!  124  ctime               btrfs_timespec
//!  136  mtime               btrfs_timespec
//!  148  otime               btrfs_timespec
//!  160  (end)
//! ```
//!
//! The struct is packed, so `mode` and the three `u32`s before it sit on
//! 4-byte boundaries only by coincidence and the timestamps sit on none
//! at all — a `btrfs_timespec` is a `u64` second count followed by a
//! `u32` nanosecond count, twelve bytes, no padding.
//!
//! # Byte order
//!
//! Little-endian throughout, like the rest of the format.
//!
//! # Confidence
//!
//! Every offset above is corroborated by real media rather than recalled:
//!
//! - The 160-byte total is confirmed twice over. Every one of the 80,004
//!   `INODE_ITEM`s across the `deep4k` and `deep16k` fixtures has a data
//!   length of exactly 160, and `struct btrfs_root_item` opens with an
//!   embedded inode item followed by `generation` and `root_dirid`,
//!   which puts its `bytenr` at 176 — the offset `tests/fstree_oracle.rs`
//!   already relies on and which yields a tree block that passes its own
//!   identity check.
//! - `mode` at 52 reads `0o40755` for the root directory of every
//!   fixture, and its top nibble is `4` (directory) for exactly the two
//!   directories in `deep4k` and `8` (regular) for the other 20,000
//!   inodes.
//! - `nlink` at 40, `size` at 16 and `nbytes` at 24 read 1, 8 and 4096
//!   for that same root directory, which is what an eight-byte, one-link
//!   directory occupying one 4 KiB block should say.
//! - The four timestamps at 112/124/136/148 all decode to seconds inside
//!   the fixture's build window with nanosecond counts below one second,
//!   across 320,016 fields. A wrong offset does not produce that.
//!
//! One thing the fixtures cannot settle is the **order of `ctime` and
//! `mtime`**. Both hold the same value on every fixture inode, so
//! swapping them would go unnoticed here; the order below is the one the
//! published field order gives, and it is called out again at the
//! constants themselves.

use crate::error::{Error, Result};
use crate::superblock::{le32, le64};

/// `BTRFS_INODE_NODATASUM` — this file's blocks carry no checksums.
pub const INODE_NODATASUM: u64 = 1 << 0;
/// `BTRFS_INODE_NODATACOW` — this file's blocks are written in place.
pub const INODE_NODATACOW: u64 = 1 << 1;

/// `BTRFS_INODE_ITEM_KEY` — the key type an inode item is filed under.
pub const INODE_ITEM_KEY: u8 = 1;

/// `BTRFS_INODE_REF_KEY` — a name-and-parent back-reference for an
/// inode. Not parsed here; named so a walk over a file tree can account
/// for every item type it meets.
pub const INODE_REF_KEY: u8 = 12;

/// Size of an on-disk `struct btrfs_inode_item`.
///
/// The field widths sum to 160: eight `u64`s and four `u32`s make 80,
/// four reserved `u64`s make another 32, and four twelve-byte timestamps
/// make 48.
pub const INODE_ITEM_SIZE: usize = 160;

/// Size of an on-disk `struct btrfs_timespec`: a `u64` of seconds and a
/// `u32` of nanoseconds, packed.
pub const TIMESPEC_SIZE: usize = 12;

/// `BTRFS_FIRST_FREE_OBJECTID` — the objectid of a subvolume's root
/// directory, and the lowest number an ordinary inode can have.
///
/// Everything below it is reserved for the trees themselves, which is
/// why a file tree's first real inode is 256 rather than 1 or 2.
pub const FIRST_FREE_OBJECTID: u64 = 256;

/// Byte offsets within a `struct btrfs_inode_item`.
///
/// Named for the same reason the superblock's and the tree header's are:
/// a bare `52` in the middle of a parse function is unreviewable, and
/// this is a packed struct where nothing about a field's position can be
/// inferred from its alignment.
pub mod offsets {
    /// `generation` — the transaction that created this inode.
    pub const GENERATION: usize = 0;
    /// `transid` — the transaction that last modified it.
    pub const TRANSID: usize = 8;
    /// `size` — file length in bytes. For a directory, the summed length
    /// of its entry names rather than an allocation.
    pub const SIZE: usize = 16;
    /// `nbytes` — bytes allocated on disk, which for a file stored
    /// inline in its own extent item is zero.
    pub const NBYTES: usize = 24;
    /// `block_group` — a hint, unused since the free-space tree.
    pub const BLOCK_GROUP: usize = 32;
    /// `nlink` — hard link count.
    pub const NLINK: usize = 40;
    /// `uid` — owning user.
    pub const UID: usize = 44;
    /// `gid` — owning group.
    pub const GID: usize = 48;
    /// `mode` — file type and permission bits. Thirty-two bits wide
    /// here, unlike the sixteen an `xfs_dinode` uses.
    pub const MODE: usize = 52;
    /// `rdev` — device number, for character and block device inodes.
    pub const RDEV: usize = 56;
    /// `flags` — `BTRFS_INODE_*` bits. This driver reads them but acts
    /// on none of them; see [`super::Inode::flags`].
    pub const FLAGS: usize = 64;
    /// `sequence` — bumped on every modification, for NFS.
    pub const SEQUENCE: usize = 72;
    /// `reserved[4]` — 32 bytes of future expansion, zero today.
    pub const RESERVED: usize = 80;
    /// `atime` — last access.
    pub const ATIME: usize = 112;
    /// `ctime` — last inode change.
    ///
    /// **`ctime` before `mtime` is the one detail in this struct that no
    /// fixture corroborates**: the two hold identical values on every
    /// inode in every fixture, so a swap here would be invisible to the
    /// oracle tests. The order is the published field order.
    pub const CTIME: usize = 124;
    /// `mtime` — last data modification. See the note on [`CTIME`].
    pub const MTIME: usize = 136;
    /// `otime` — creation ("origin") time.
    pub const OTIME: usize = 148;
}

/// Nanoseconds in a second. A `btrfs_timespec` written by the kernel is
/// always normalised, so anything at or above this is a decoding error
/// rather than an unusual timestamp — see [`Timestamp::parse`].
const NSEC_PER_SEC: u32 = 1_000_000_000;

/// Mask selecting the file-type bits of `mode` (`S_IFMT`).
const S_IFMT: u32 = 0xF000;

/// Mask selecting the permission and set-id bits of `mode`.
const S_IPERM: u32 = 0o7777;

/// File type, decoded from the mode's format bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Regular file.
    Regular,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Character device.
    CharDevice,
    /// Block device.
    BlockDevice,
    /// FIFO.
    Fifo,
    /// Unix domain socket.
    Socket,
}

impl FileType {
    /// Decode a type from the `S_IFMT` bits of a mode word.
    ///
    /// Returns `None` for a value that is not one of the seven POSIX
    /// types, rather than guessing at one.
    pub fn from_mode(mode: u32) -> Option<Self> {
        Some(match mode & S_IFMT {
            0x8000 => FileType::Regular,
            0x4000 => FileType::Directory,
            0xA000 => FileType::Symlink,
            0x2000 => FileType::CharDevice,
            0x6000 => FileType::BlockDevice,
            0x1000 => FileType::Fifo,
            0xC000 => FileType::Socket,
            _ => return None,
        })
    }
}

/// One `struct btrfs_timespec`.
///
/// Btrfs has only ever had this representation — there is no `bigtime`
/// equivalent to the one XFS grew, because the second count was 64 bits
/// from the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timestamp {
    /// Seconds since the Unix epoch. Stored unsigned on disk but kept
    /// signed here, because the kernel writes a signed `time64_t` into
    /// it and a pre-1970 timestamp therefore appears as a very large
    /// unsigned value.
    pub sec: i64,
    /// Nanoseconds within the second, always below one second.
    pub nsec: u32,
}

impl Timestamp {
    /// Decode a timestamp from `buf` at `off`.
    ///
    /// # Errors
    ///
    /// [`Error::BadSuperblock`] if the nanosecond field is a second or
    /// more. The on-disk format does not formally forbid that, but every
    /// timestamp the kernel writes comes from a normalised `timespec64`,
    /// so an out-of-range nanosecond count is the loudest signal
    /// available that these twelve bytes are not a timestamp at all —
    /// which is exactly what a wrong offset in a packed struct produces.
    /// Refusing is the honest response: the alternative is to hand back
    /// a plausible-looking date derived from the wrong bytes.
    fn parse(buf: &[u8], off: usize, what: &str) -> Result<Self> {
        let nsec = le32(buf, off + 8);
        if nsec >= NSEC_PER_SEC {
            return Err(Error::BadSuperblock(format!(
                "inode {what} has {nsec} nanoseconds, which is a second or more — \
                 the timestamp offsets are wrong"
            )));
        }
        Ok(Timestamp {
            sec: le64(buf, off) as i64,
            nsec,
        })
    }
}

/// A parsed `struct btrfs_inode_item`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inode {
    /// Inode number. Taken from the key the item was filed under, not
    /// from the item itself, which does not record it.
    pub ino: u64,
    /// Transaction that created this inode.
    pub generation: u64,
    /// Transaction that last modified it.
    pub transid: u64,
    /// File length in bytes.
    pub size: u64,
    /// Bytes allocated on disk. Zero for a file small enough to live
    /// inside its own extent item.
    pub nbytes: u64,
    /// Block group hint. Vestigial; kept because it is a real field and
    /// omitting it would leave a silent gap in the struct.
    pub block_group: u64,
    /// Hard link count.
    pub nlink: u32,
    /// Owning user id.
    pub uid: u32,
    /// Owning group id.
    pub gid: u32,
    /// Raw mode, including both the type and the permission bits.
    pub mode: u32,
    /// Device number, meaningful only for a character or block device.
    pub rdev: u64,
    /// `BTRFS_INODE_*` flag bits, exactly as stored.
    ///
    /// The read path does not interpret them: whether a file's data is
    /// compressed, for instance, is decided per extent rather than per
    /// inode. The write path does -- a file is written in place only when
    /// it carries [`INODE_NODATASUM`] and [`INODE_NODATACOW`], which are
    /// defined here beside the field they decode.
    pub flags: u64,
    /// Modification sequence number, for NFS.
    pub sequence: u64,
    /// Last access time.
    pub atime: Timestamp,
    /// Last inode-change time.
    pub ctime: Timestamp,
    /// Last data-modification time.
    pub mtime: Timestamp,
    /// Creation time.
    pub otime: Timestamp,
}

impl Inode {
    /// Parse the inode item filed under objectid `ino`.
    ///
    /// `data` is the item's data as returned by the B-tree walker. It
    /// must be at least [`INODE_ITEM_SIZE`] bytes; anything past that is
    /// ignored, because the struct is explicitly reserved room to grow
    /// and a longer item from a future kernel is not a reason to refuse
    /// a filesystem that is otherwise readable.
    ///
    /// # Errors
    ///
    /// [`Error::BadSuperblock`] if the item is too short to hold an
    /// inode, or if a timestamp does not decode as one.
    pub fn parse(data: &[u8], ino: u64) -> Result<Self> {
        use offsets as o;
        if data.len() < INODE_ITEM_SIZE {
            return Err(Error::BadSuperblock(format!(
                "inode {ino}: item is {} bytes, too short for the {INODE_ITEM_SIZE}-byte inode item",
                data.len()
            )));
        }
        Ok(Inode {
            ino,
            generation: le64(data, o::GENERATION),
            transid: le64(data, o::TRANSID),
            size: le64(data, o::SIZE),
            nbytes: le64(data, o::NBYTES),
            block_group: le64(data, o::BLOCK_GROUP),
            nlink: le32(data, o::NLINK),
            uid: le32(data, o::UID),
            gid: le32(data, o::GID),
            mode: le32(data, o::MODE),
            rdev: le64(data, o::RDEV),
            flags: le64(data, o::FLAGS),
            sequence: le64(data, o::SEQUENCE),
            atime: Timestamp::parse(data, o::ATIME, "atime")?,
            ctime: Timestamp::parse(data, o::CTIME, "ctime")?,
            mtime: Timestamp::parse(data, o::MTIME, "mtime")?,
            otime: Timestamp::parse(data, o::OTIME, "otime")?,
        })
    }

    /// Decode the file type from the mode's format bits.
    ///
    /// `None` means the bits name no POSIX type, which is a corrupt
    /// inode rather than an exotic one.
    pub fn file_type(&self) -> Option<FileType> {
        FileType::from_mode(self.mode)
    }

    /// Permission and set-id bits only.
    pub fn permissions(&self) -> u32 {
        self.mode & S_IPERM
    }

    /// Whether this inode is a directory.
    pub fn is_dir(&self) -> bool {
        self.file_type() == Some(FileType::Directory)
    }

    /// Whether this inode is a regular file.
    pub fn is_regular_file(&self) -> bool {
        self.file_type() == Some(FileType::Regular)
    }

    /// Whether this inode is a symbolic link.
    ///
    /// A Btrfs symlink stores its target the same way a small file
    /// stores its contents — as an inline extent — so reading one goes
    /// through the ordinary file path.
    pub fn is_symlink(&self) -> bool {
        self.file_type() == Some(FileType::Symlink)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests over hand-built inode items.
    //!
    //! **Necessary but not sufficient.** Every item below is encoded by
    //! this module using the same offsets and byte order the parser
    //! decodes with, so a misreading of `struct btrfs_inode_item` would
    //! be baked into both sides and these tests would pass anyway. What
    //! they buy is coverage of the bounds arithmetic, the mode decoding
    //! and the timestamp validation, which are right or wrong
    //! independently of where the fields sit. Whether the offsets
    //! themselves are right is settled by `tests/fs_oracle.rs`, which
    //! reads inodes that `mkfs.btrfs` and the Linux kernel wrote.

    use super::*;

    /// A 160-byte inode item with every field set to something
    /// distinguishable, so a transposed pair of offsets shows up as a
    /// wrong value rather than as a coincidence.
    fn item(mode: u32) -> Vec<u8> {
        let mut b = vec![0u8; INODE_ITEM_SIZE];
        let put64 = |b: &mut Vec<u8>, at: usize, v: u64| {
            b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        };
        let put32 = |b: &mut Vec<u8>, at: usize, v: u32| {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        };
        put64(&mut b, offsets::GENERATION, 11);
        put64(&mut b, offsets::TRANSID, 22);
        put64(&mut b, offsets::SIZE, 33);
        put64(&mut b, offsets::NBYTES, 4096);
        put64(&mut b, offsets::BLOCK_GROUP, 55);
        put32(&mut b, offsets::NLINK, 3);
        put32(&mut b, offsets::UID, 1000);
        put32(&mut b, offsets::GID, 1001);
        put32(&mut b, offsets::MODE, mode);
        put64(&mut b, offsets::RDEV, 77);
        put64(&mut b, offsets::FLAGS, 0x88);
        put64(&mut b, offsets::SEQUENCE, 99);
        for (i, off) in [
            offsets::ATIME,
            offsets::CTIME,
            offsets::MTIME,
            offsets::OTIME,
        ]
        .into_iter()
        .enumerate()
        {
            put64(&mut b, off, 1_700_000_000 + i as u64);
            put32(&mut b, off + 8, 100 + i as u32);
        }
        b
    }

    #[test]
    fn parses_every_field() {
        let inode = Inode::parse(&item(0o100644), 300).unwrap();
        assert_eq!(inode.ino, 300);
        assert_eq!(inode.generation, 11);
        assert_eq!(inode.transid, 22);
        assert_eq!(inode.size, 33);
        assert_eq!(inode.nbytes, 4096);
        assert_eq!(inode.block_group, 55);
        assert_eq!(inode.nlink, 3);
        assert_eq!(inode.uid, 1000);
        assert_eq!(inode.gid, 1001);
        assert_eq!(inode.mode, 0o100644);
        assert_eq!(inode.rdev, 77);
        assert_eq!(inode.flags, 0x88);
        assert_eq!(inode.sequence, 99);
    }

    /// Each timestamp must come from its own twelve bytes. Giving them
    /// distinct values catches an off-by-one-field slip that identical
    /// values would hide.
    #[test]
    fn timestamps_are_read_from_distinct_slots() {
        let inode = Inode::parse(&item(0o100644), 300).unwrap();
        assert_eq!(
            inode.atime,
            Timestamp {
                sec: 1_700_000_000,
                nsec: 100
            }
        );
        assert_eq!(
            inode.ctime,
            Timestamp {
                sec: 1_700_000_001,
                nsec: 101
            }
        );
        assert_eq!(
            inode.mtime,
            Timestamp {
                sec: 1_700_000_002,
                nsec: 102
            }
        );
        assert_eq!(
            inode.otime,
            Timestamp {
                sec: 1_700_000_003,
                nsec: 103
            }
        );
    }

    #[test]
    fn decodes_the_seven_file_types() {
        for (bits, want) in [
            (0o100000u32, FileType::Regular),
            (0o040000, FileType::Directory),
            (0o120000, FileType::Symlink),
            (0o020000, FileType::CharDevice),
            (0o060000, FileType::BlockDevice),
            (0o010000, FileType::Fifo),
            (0o140000, FileType::Socket),
        ] {
            let inode = Inode::parse(&item(bits | 0o644), 300).unwrap();
            assert_eq!(inode.file_type(), Some(want), "mode {bits:o}");
            assert_eq!(inode.permissions(), 0o644);
        }
    }

    #[test]
    fn an_undefined_type_nibble_decodes_to_nothing() {
        let inode = Inode::parse(&item(0o030644), 300).unwrap();
        assert_eq!(inode.file_type(), None);
        assert!(!inode.is_dir() && !inode.is_regular_file() && !inode.is_symlink());
    }

    #[test]
    fn type_predicates_agree_with_the_decoded_type() {
        assert!(Inode::parse(&item(0o040755), 256).unwrap().is_dir());
        assert!(Inode::parse(&item(0o100644), 257)
            .unwrap()
            .is_regular_file());
        assert!(Inode::parse(&item(0o120777), 258).unwrap().is_symlink());
    }

    /// The set-id and sticky bits are permissions, not type bits.
    #[test]
    fn permissions_keep_the_set_id_and_sticky_bits() {
        let inode = Inode::parse(&item(0o104755), 300).unwrap();
        assert_eq!(inode.permissions(), 0o4755);
        assert_eq!(inode.file_type(), Some(FileType::Regular));
    }

    #[test]
    fn rejects_an_item_too_short_to_be_an_inode() {
        let short = vec![0u8; INODE_ITEM_SIZE - 1];
        match Inode::parse(&short, 300) {
            Err(Error::BadSuperblock(m)) => assert!(m.contains("300"), "message lost the ino: {m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// A longer item is a future kernel's inode, not a corrupt one.
    #[test]
    fn accepts_an_item_longer_than_todays_struct() {
        let mut long = item(0o100644);
        long.extend_from_slice(&[0xAB; 32]);
        assert_eq!(Inode::parse(&long, 300).unwrap().size, 33);
    }

    /// A nanosecond count of a second or more means these bytes are not
    /// a timestamp, which is what a wrong offset looks like.
    #[test]
    fn rejects_a_nanosecond_count_of_a_second_or_more() {
        for (off, name) in [
            (offsets::ATIME, "atime"),
            (offsets::CTIME, "ctime"),
            (offsets::MTIME, "mtime"),
            (offsets::OTIME, "otime"),
        ] {
            let mut b = item(0o100644);
            b[off + 8..off + 12].copy_from_slice(&NSEC_PER_SEC.to_le_bytes());
            match Inode::parse(&b, 300) {
                Err(Error::BadSuperblock(m)) => {
                    assert!(m.contains(name), "message did not name {name}: {m}")
                }
                other => panic!("expected a refusal for {name}, got {other:?}"),
            }
        }
    }

    /// A second count past 2^63 is a pre-1970 date, not an error.
    #[test]
    fn a_second_count_above_the_signed_range_reads_as_a_negative_date() {
        let mut b = item(0o100644);
        b[offsets::ATIME..offsets::ATIME + 8].copy_from_slice(&(-1i64).to_le_bytes());
        assert_eq!(Inode::parse(&b, 300).unwrap().atime.sec, -1);
    }

    /// The struct's own arithmetic: the four timestamps must exactly
    /// fill the tail of the item.
    #[test]
    fn the_timestamps_end_exactly_at_the_end_of_the_item() {
        assert_eq!(offsets::OTIME + TIMESPEC_SIZE, INODE_ITEM_SIZE);
        assert_eq!(offsets::CTIME - offsets::ATIME, TIMESPEC_SIZE);
        assert_eq!(offsets::MTIME - offsets::CTIME, TIMESPEC_SIZE);
        assert_eq!(offsets::OTIME - offsets::MTIME, TIMESPEC_SIZE);
        assert_eq!(offsets::ATIME - offsets::RESERVED, 4 * 8);
    }
}
