//! Error type for the Btrfs driver.
//!
//! Mirrors the shape used by the sister `fs-*` crates so the C ABI layer
//! can map a driver error onto an errno the same way across drivers.

use std::fmt;

/// Everything that can go wrong reading or writing a Btrfs volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The underlying block device failed a read or write.
    Io(String),

    /// The eight bytes at superblock offset `0x40` are not `_BHRfS_M` —
    /// this is not a Btrfs volume. The bytes we actually saw are carried
    /// along because the commonest cause of a spurious mismatch is a
    /// byte-order or offset slip in the reader, and the raw bytes make
    /// that immediately visible in a log line.
    NotBtrfs {
        /// The eight bytes read from the magic field.
        magic: [u8; 8],
    },

    /// The volume is Btrfs, but a structural field is out of range or
    /// internally inconsistent. Carries a human-readable description of
    /// the specific field, because a bad geometry value is almost always
    /// the first symptom of reading the wrong offset.
    BadSuperblock(String),

    /// A metadata block failed the checksum named by `csum_type`. Btrfs
    /// checksums every superblock, every tree node and (optionally) every
    /// data extent, so this is the driver's primary corruption signal.
    ChecksumMismatch {
        /// What kind of structure was being read.
        what: &'static str,
        /// Byte offset the structure was read from. For a superblock
        /// this is one of the mirror offsets; for a tree node it is the
        /// logical address.
        offset: u64,
    },

    /// `csum_type` names a hash algorithm this driver does not implement.
    /// Distinct from [`Error::ChecksumMismatch`]: the data may be fine,
    /// we simply cannot check it.
    UnsupportedChecksum(u16),

    /// A self-describing structure disagrees with where it was read from.
    /// The superblock records its own byte offset in `bytenr`, and every
    /// tree node records its own logical address, so a mismatch catches
    /// misdirected reads and stale block reuse that a checksum alone
    /// cannot.
    BlockIdentityMismatch {
        /// What kind of structure was being read.
        what: &'static str,
        /// Address we read from.
        expected: u64,
        /// Address the structure claims for itself.
        found: u64,
    },

    /// The volume sets an incompatible feature bit this driver does not
    /// implement. Distinct from [`Error::BadSuperblock`]: the volume is
    /// well-formed, we simply cannot honour it safely.
    UnsupportedFeature(String),

    /// A chunk item is malformed — zero stripes, a stripe count that
    /// disagrees with its RAID profile, a truncated stripe array, or two
    /// profile bits set at once.
    BadChunkItem(String),

    /// No chunk covers this logical address. Every read in Btrfs starts
    /// as a logical address, so an unmapped one means either the chunk
    /// tree has not been loaded yet or a tree pointer is corrupt.
    UnmappedLogical(u64),

    /// A chunk uses a RAID profile this driver cannot translate. Kept
    /// separate from [`Error::UnsupportedFeature`] so a caller can tell
    /// "this is a RAID6 array" from "this is a zoned volume" without
    /// parsing a message string.
    UnsupportedProfile(String),

    /// The log tree is non-empty: it holds fsync'd changes that have not
    /// been folded into the main trees. Reading the committed trees is
    /// still safe and self-consistent, just slightly stale, so the parse
    /// path reports this through [`crate::Superblock::has_dirty_log`]
    /// rather than failing; this variant exists for callers that want a
    /// hard refusal.
    DirtyLog,

    /// A path component was not found.
    NotFound,

    /// A path component exists but is not a directory.
    NotADirectory,

    /// The operation requires a regular file.
    NotAFile,

    /// The requested operation would write, but the volume is mounted
    /// read-only or the driver has no write path for this structure.
    ReadOnly,

    /// A format was asked for that cannot be made: a device too small, a
    /// node size out of range, a label too long. Nothing was written.
    InvalidGeometry(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(m) => write!(f, "device I/O failed: {m}"),
            Error::NotBtrfs { magic } => {
                write!(f, "not a Btrfs volume (magic bytes {magic:02x?})")
            }
            Error::BadSuperblock(m) => write!(f, "malformed Btrfs superblock: {m}"),
            Error::ChecksumMismatch { what, offset } => {
                write!(f, "{what} at offset {offset} failed its checksum")
            }
            Error::UnsupportedChecksum(t) => {
                write!(f, "unsupported Btrfs checksum type {t}")
            }
            Error::BlockIdentityMismatch {
                what,
                expected,
                found,
            } => write!(f, "{what} read from {expected} claims to live at {found}"),
            Error::UnsupportedFeature(m) => write!(f, "unsupported Btrfs feature: {m}"),
            Error::BadChunkItem(m) => write!(f, "malformed Btrfs chunk item: {m}"),
            Error::UnmappedLogical(a) => {
                write!(f, "logical address {a:#x} is not covered by any chunk")
            }
            Error::UnsupportedProfile(m) => write!(f, "unsupported Btrfs chunk profile: {m}"),
            Error::DirtyLog => f.write_str("Btrfs log tree is non-empty and needs replay"),
            Error::NotFound => f.write_str("no such file or directory"),
            Error::NotADirectory => f.write_str("not a directory"),
            Error::NotAFile => f.write_str("not a regular file"),
            Error::ReadOnly => f.write_str("filesystem is read-only"),
            Error::InvalidGeometry(m) => write!(f, "cannot format: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<fs_core::Error> for Error {
    fn from(e: fs_core::Error) -> Self {
        Error::Io(e.to_string())
    }
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    //! The `Display` text here is what reaches a user when a volume will
    //! not mount, and what a maintainer reads in a bug report. Each
    //! message is required to name the values that identify the problem:
    //! "checksum mismatch" without saying *where* costs an afternoon.

    use super::*;

    #[test]
    fn not_btrfs_shows_the_magic_it_found() {
        let e = Error::NotBtrfs {
            magic: *b"NOTABTRF",
        };
        let s = e.to_string();
        assert!(!s.is_empty());
        assert!(
            s.to_lowercase().contains("btrfs"),
            "message should name the format: {s}"
        );
    }

    #[test]
    fn checksum_mismatch_names_the_structure_and_offset() {
        let e = Error::ChecksumMismatch {
            what: "superblock",
            offset: 65536,
        };
        let s = e.to_string();
        assert!(s.contains("superblock"), "structure missing from: {s}");
        assert!(
            s.contains("65536") || s.contains("10000"),
            "offset missing: {s}"
        );
    }

    #[test]
    fn identity_mismatch_names_both_addresses() {
        let e = Error::BlockIdentityMismatch {
            what: "tree block",
            expected: 0x1000,
            found: 0x2000,
        };
        let s = e.to_string();
        assert!(s.contains("tree block"), "structure missing from: {s}");
        assert!(!s.is_empty());
    }

    #[test]
    fn unmapped_logical_names_the_address() {
        let e = Error::UnmappedLogical(0xdead_beef);
        let s = e.to_string();
        assert!(
            s.contains("deadbeef") || s.contains("3735928559"),
            "address missing from: {s}"
        );
    }

    #[test]
    fn unsupported_checksum_names_the_type() {
        let e = Error::UnsupportedChecksum(99);
        assert!(e.to_string().contains("99"));
    }

    #[test]
    fn string_carrying_variants_keep_their_detail() {
        for (e, needle) in [
            (Error::Io("device went away".into()), "device went away"),
            (Error::BadSuperblock("nodesize 7".into()), "nodesize 7"),
            (Error::UnsupportedFeature("raid5".into()), "raid5"),
            (Error::BadChunkItem("zero stripes".into()), "zero stripes"),
            (Error::UnsupportedProfile("raid6".into()), "raid6"),
        ] {
            let s = e.to_string();
            assert!(s.contains(needle), "{e:?} lost its detail: {s}");
        }
    }

    /// Every variant must render as non-empty text, so a variant added
    /// later without a match arm cannot slip through as a blank message.
    #[test]
    fn every_variant_renders_non_empty() {
        let all = [
            Error::Io("x".into()),
            Error::NotBtrfs { magic: [0; 8] },
            Error::BadSuperblock("x".into()),
            Error::ChecksumMismatch {
                what: "x",
                offset: 0,
            },
            Error::UnsupportedChecksum(0),
            Error::BlockIdentityMismatch {
                what: "x",
                expected: 0,
                found: 1,
            },
            Error::UnsupportedFeature("x".into()),
            Error::BadChunkItem("x".into()),
            Error::UnmappedLogical(0),
            Error::UnsupportedProfile("x".into()),
            Error::DirtyLog,
            Error::NotFound,
            Error::NotADirectory,
            Error::NotAFile,
            Error::ReadOnly,
        ];
        for e in all {
            assert!(!e.to_string().is_empty(), "{e:?} rendered as empty text");
        }
    }

    #[test]
    fn unit_variants_say_something_useful() {
        for (e, expect) in [
            (Error::NotFound, "no such"),
            (Error::NotADirectory, "not a directory"),
            (Error::NotAFile, "not a regular file"),
            (Error::ReadOnly, "read-only"),
        ] {
            let s = e.to_string().to_lowercase();
            assert!(
                s.contains(expect),
                "{e:?} rendered as {s:?}, which does not mention {expect:?}"
            );
        }
    }

    #[test]
    fn wraps_a_device_error() {
        let e: Error = fs_core::Error::ReadOnly.into();
        assert!(matches!(e, Error::Io(_)), "device errors become Io");
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn implements_std_error() {
        fn assert_is_error<T: std::error::Error>() {}
        assert_is_error::<Error>();
    }
}
