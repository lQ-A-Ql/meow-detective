//! Btrfs superblock parsing and validation.
//!
//! # Byte order
//!
//! **Btrfs stores every multi-byte on-disk field little-endian**, on
//! every host, including the checksum. That uniformity is worth stating
//! explicitly because the sister XFS driver in this family is the exact
//! opposite — big-endian everywhere *except* its CRC, which is stored
//! little-endian. A reader that carries the XFS habit across will get
//! Btrfs wrong in a way no round-trip test can see: a writer and reader
//! that share a byte-order mistake agree with each other perfectly while
//! disagreeing with every real filesystem.
//!
//! # Layout
//!
//! The superblock is a fixed [`SUPER_INFO_SIZE`]-byte (4 KiB) structure
//! written at up to three fixed *physical* offsets — see
//! [`SUPER_OFFSETS`]. Copy 0 at 64 KiB is the primary; a copy is only
//! written when the device is large enough to hold it. Every copy is
//! self-describing: `bytenr` records the offset the copy belongs at, so a
//! misdirected read is detectable independently of the checksum.
//!
//! # Checksum
//!
//! The first [`CSUM_SIZE`] (32) bytes hold the checksum. It covers bytes
//! `0x20 .. 0x1000` — that is, the whole 4 KiB superblock *minus* the
//! checksum field itself, not merely the fields this driver happens to
//! parse and not merely up to the end of some sub-structure. Getting that
//! span wrong is invisible to a self-built fixture and fatal on real
//! media.
//!
//! Only the first `csum_type.digest_len()` bytes of the 32-byte field are
//! meaningful; the writer zero-fills the rest and the kernel compares
//! only the meaningful prefix. This driver does the same.
//!
//! # Confidence
//!
//! Every offset in [`offsets`] was taken from the published on-disk
//! format tables and cross-checked against the field order of
//! `struct btrfs_super_block`. Anything that could not be corroborated by
//! two independent sources is called out in a comment at its use site.

use crate::error::{Error, Result};

/// The eight magic bytes at [`offsets::MAGIC`], in on-disk order.
///
/// Spelled as bytes rather than as an integer on purpose. Btrfs's magic
/// is an ASCII string, and writing it as a `u64` invites exactly the
/// transposition bug that is impossible to spot in a hand-built fixture:
/// the fixture writer and the parser share the mistake and agree.
/// [`BTRFS_MAGIC_LE64`] records the little-endian integer form for
/// cross-reference, and a unit test asserts the two agree.
pub const BTRFS_MAGIC: [u8; 8] = *b"_BHRfS_M";

/// [`BTRFS_MAGIC`] as the little-endian `u64` the format documentation
/// also quotes it as: `0x4D5F_5366_5248_425F`.
pub const BTRFS_MAGIC_LE64: u64 = 0x4D5F_5366_5248_425F;

/// Size of the on-disk superblock structure in bytes (`BTRFS_SUPER_INFO_SIZE`).
pub const SUPER_INFO_SIZE: usize = 4096;

/// Physical byte offset of the primary superblock (64 KiB).
pub const SUPER_INFO_OFFSET: u64 = 0x1_0000;

/// Every physical offset a superblock copy may live at.
///
/// Copy 0 is the primary at 64 KiB; copies 1 and 2 are at 64 MiB and
/// 256 GiB. The kernel derives these as `16 KiB << (12 * mirror)` for
/// `mirror > 0` and caps the count at three, so there is deliberately no
/// 1 PiB copy even though the shift sequence would produce one.
pub const SUPER_OFFSETS: [u64; 3] = [SUPER_INFO_OFFSET, 0x400_0000, 0x40_0000_0000];

/// Size of the checksum field (`BTRFS_CSUM_SIZE`). Wide enough for the
/// largest supported digest; narrower digests are zero-padded.
pub const CSUM_SIZE: usize = 32;

/// Size of an FS/device UUID field (`BTRFS_FSID_SIZE` / `BTRFS_UUID_SIZE`).
pub const UUID_SIZE: usize = 16;

/// Size of the label field (`BTRFS_LABEL_SIZE`), NUL-padded.
pub const LABEL_SIZE: usize = 256;

/// Size of the embedded system chunk array (`BTRFS_SYSTEM_CHUNK_ARRAY_SIZE`).
pub const SYS_CHUNK_ARRAY_SIZE: usize = 2048;

/// Size of the embedded `struct btrfs_dev_item`.
pub const DEV_ITEM_SIZE: usize = 0x62;

/// Maximum B-tree height (`BTRFS_MAX_LEVEL`). A `*_level` field at or
/// above this is nonsense.
pub const MAX_LEVEL: u8 = 8;

/// Largest metadata block size Btrfs permits (`BTRFS_MAX_METADATA_BLOCKSIZE`).
pub const MAX_METADATA_BLOCKSIZE: u32 = 65536;

/// Smallest sector size Btrfs permits.
pub const MIN_SECTORSIZE: u32 = 4096;

/// Byte offsets of every superblock field, relative to the start of the
/// structure.
///
/// Kept as named constants rather than inlined numbers so that a reader
/// can diff this block against the format documentation directly, and so
/// that a cross-validation harness can assert against the same names the
/// parser uses.
pub mod offsets {
    /// Checksum of bytes `0x20 .. 0x1000`.
    pub const CSUM: usize = 0x00;
    /// Filesystem UUID.
    pub const FSID: usize = 0x20;
    /// Physical byte offset this superblock copy belongs at.
    pub const BYTENR: usize = 0x30;
    /// Superblock flags — see [`super::super_flags`].
    pub const FLAGS: usize = 0x38;
    /// `_BHRfS_M`.
    pub const MAGIC: usize = 0x40;
    /// Transaction id that wrote this superblock.
    pub const GENERATION: usize = 0x48;
    /// Logical address of the root tree's root node.
    pub const ROOT: usize = 0x50;
    /// Logical address of the chunk tree's root node.
    pub const CHUNK_ROOT: usize = 0x58;
    /// Logical address of the log tree's root node, 0 when clean.
    pub const LOG_ROOT: usize = 0x60;
    /// Legacy `log_root_transid`; unused by modern kernels.
    pub const LOG_ROOT_TRANSID: usize = 0x68;
    /// Total size of the filesystem across all devices.
    pub const TOTAL_BYTES: usize = 0x70;
    /// Bytes currently allocated.
    pub const BYTES_USED: usize = 0x78;
    /// Objectid of the root directory (conventionally 6).
    pub const ROOT_DIR_OBJECTID: usize = 0x80;
    /// Number of devices in the filesystem.
    pub const NUM_DEVICES: usize = 0x88;
    /// Sector size in bytes.
    pub const SECTORSIZE: usize = 0x90;
    /// Metadata node size in bytes.
    pub const NODESIZE: usize = 0x94;
    /// Legacy `leafsize`; modern kernels call this `__unused_leafsize`
    /// and mkfs writes `nodesize` into it.
    pub const LEAFSIZE: usize = 0x98;
    /// Stripe size in bytes; modern kernels require it to equal
    /// `sectorsize`.
    pub const STRIPESIZE: usize = 0x9c;
    /// Valid byte count within [`SYS_CHUNK_ARRAY`].
    pub const SYS_CHUNK_ARRAY_SIZE: usize = 0xa0;
    /// Transaction id of the chunk tree root.
    pub const CHUNK_ROOT_GENERATION: usize = 0xa4;
    /// Compatible feature mask.
    pub const COMPAT_FLAGS: usize = 0xac;
    /// Read-only-compatible feature mask.
    pub const COMPAT_RO_FLAGS: usize = 0xb4;
    /// Incompatible feature mask.
    pub const INCOMPAT_FLAGS: usize = 0xbc;
    /// Metadata checksum algorithm selector.
    pub const CSUM_TYPE: usize = 0xc4;
    /// Height of the root tree.
    pub const ROOT_LEVEL: usize = 0xc6;
    /// Height of the chunk tree.
    pub const CHUNK_ROOT_LEVEL: usize = 0xc7;
    /// Height of the log tree.
    pub const LOG_ROOT_LEVEL: usize = 0xc8;
    /// Embedded `struct btrfs_dev_item` describing *this* device.
    pub const DEV_ITEM: usize = 0xc9;
    /// NUL-padded volume label.
    pub const LABEL: usize = 0x12b;
    /// Free-space-cache generation.
    pub const CACHE_GENERATION: usize = 0x22b;
    /// UUID tree generation.
    pub const UUID_TREE_GENERATION: usize = 0x233;
    /// UUID stamped into tree node headers when the `METADATA_UUID`
    /// incompatible feature is set.
    ///
    /// This one field sits in what the older published format tables
    /// still describe as a 240-byte reserved run. Its position is derived
    /// from the field order of `struct btrfs_super_block`, and the
    /// arithmetic closes exactly: `0x23b + 16` (metadata_uuid) `+ 8`
    /// (nr_global_roots) `+ 216` (the remaining reserved bytes) lands on
    /// [`SYS_CHUNK_ARRAY`] at `0x32b`. Worth confirming against a real
    /// `mkfs.btrfs -U`/`btrfstune -m` image all the same.
    pub const METADATA_UUID: usize = 0x23b;
    /// Number of global roots (extent-tree-v2 era). Zero on every volume
    /// this driver will accept.
    pub const NR_GLOBAL_ROOTS: usize = 0x24b;
    /// Bootstrap chunk mappings — `(key, chunk item)` pairs.
    pub const SYS_CHUNK_ARRAY: usize = 0x32b;
    /// Four backup root records.
    pub const SUPER_ROOTS: usize = 0xb2b;
}

/// Byte offsets within the embedded `struct btrfs_dev_item`, relative to
/// [`offsets::DEV_ITEM`].
pub mod dev_item_offsets {
    /// Device id.
    pub const DEVID: usize = 0x00;
    /// Device capacity in bytes.
    pub const TOTAL_BYTES: usize = 0x08;
    /// Bytes allocated on this device.
    pub const BYTES_USED: usize = 0x10;
    /// Optimal I/O alignment.
    pub const IO_ALIGN: usize = 0x18;
    /// Optimal I/O width.
    pub const IO_WIDTH: usize = 0x1c;
    /// Minimal I/O size.
    pub const SECTOR_SIZE: usize = 0x20;
    /// Device type flags.
    pub const TYPE: usize = 0x24;
    /// Generation.
    pub const GENERATION: usize = 0x2c;
    /// Start offset — bytes at the front of the device left unused.
    pub const START_OFFSET: usize = 0x34;
    /// Device group.
    pub const DEV_GROUP: usize = 0x3c;
    /// Seek speed hint.
    pub const SEEK_SPEED: usize = 0x40;
    /// Bandwidth hint.
    pub const BANDWIDTH: usize = 0x41;
    /// Device UUID.
    pub const UUID: usize = 0x42;
    /// UUID of the filesystem this device belongs to.
    pub const FSID: usize = 0x52;
}

/// Superblock `flags` bits.
pub mod super_flags {
    /// The filesystem was marked as having hit an error.
    pub const ERROR: u64 = 1 << 2;
    /// This device is a read-only seed for another filesystem.
    pub const SEEDING: u64 = 1 << 32;
    /// Image produced by a metadata-only dump tool; data extents are
    /// absent, so the volume is not mountable.
    pub const METADUMP: u64 = 1 << 33;
    /// Second-generation metadata-only dump.
    pub const METADUMP_V2: u64 = 1 << 34;
    /// A `fsid` change is in flight.
    pub const CHANGING_FSID: u64 = 1 << 35;
    /// A `fsid` change is in flight (v2 scheme).
    pub const CHANGING_FSID_V2: u64 = 1 << 36;
}

/// `incompat_flags` bits. A volume setting a bit this driver does not
/// understand cannot be read at all, let alone written.
pub mod incompat {
    /// Extent back-references use the mixed format. Universal since 2.6.31.
    pub const MIXED_BACKREF: u64 = 1 << 0;
    /// A non-default subvolume is set as the mount default.
    pub const DEFAULT_SUBVOL: u64 = 1 << 1;
    /// Data and metadata share block groups (small-volume layout).
    pub const MIXED_GROUPS: u64 = 1 << 2;
    /// Some extents may be LZO-compressed.
    pub const COMPRESS_LZO: u64 = 1 << 3;
    /// Some extents may be zstd-compressed.
    pub const COMPRESS_ZSTD: u64 = 1 << 4;
    /// Metadata nodes may be larger than one page. Universal since 3.4.
    pub const BIG_METADATA: u64 = 1 << 5;
    /// Extended inode references (long/many hard links).
    pub const EXTENDED_IREF: u64 = 1 << 6;
    /// The volume contains RAID5 or RAID6 chunks.
    pub const RAID56: u64 = 1 << 7;
    /// Metadata back-references use the compact "skinny" key form.
    pub const SKINNY_METADATA: u64 = 1 << 8;
    /// File holes are implicit — no explicit hole extents are recorded.
    pub const NO_HOLES: u64 = 1 << 9;
    /// Tree nodes are stamped with `metadata_uuid` instead of `fsid`.
    pub const METADATA_UUID: u64 = 1 << 10;
    /// RAID1C3 and RAID1C4 (three- and four-way mirror) profiles.
    pub const RAID1C34: u64 = 1 << 11;
    /// Zoned-device layout — sequential-write zones, no in-place update.
    pub const ZONED: u64 = 1 << 12;
    /// Second-generation extent tree (global roots, per-block-group trees).
    pub const EXTENT_TREE_V2: u64 = 1 << 13;
    /// Separate tree recording RAID stripe placement.
    pub const RAID_STRIPE_TREE: u64 = 1 << 14;
    /// Simple (non-hierarchical) quota accounting.
    ///
    /// Bit 15 is unassigned; simple quotas take bit 16. Both this and
    /// [`REMAP_TREE`] are recent enough that they are worth re-checking
    /// against the kernel a reader is expected to interoperate with.
    pub const SIMPLE_QUOTA: u64 = 1 << 16;
    /// Device remap tree.
    pub const REMAP_TREE: u64 = 1 << 17;

    /// Every incompatible feature this driver can read.
    ///
    /// The exclusions are deliberate, not accidental:
    ///
    /// - [`RAID56`] — parity reconstruction is not implemented, and
    ///   guessing at a RAID5 stripe layout is worse than refusing.
    /// - [`ZONED`], [`EXTENT_TREE_V2`], [`RAID_STRIPE_TREE`],
    ///   [`SIMPLE_QUOTA`], [`REMAP_TREE`] — each restructures something
    ///   the read path depends on.
    ///
    /// [`COMPRESS_LZO`] and [`COMPRESS_ZSTD`] ARE accepted, which was
    /// not always so. The concern that kept them out was a reader
    /// handing back compressed bytes as if they were file contents — a
    /// failure a caller cannot detect. That risk is now handled where it
    /// actually arises: the extent decoder refuses a compressed extent
    /// by name, so a compressed file errors and an uncompressed one
    /// reads normally.
    ///
    /// Refusing the whole mount instead was the worse trade. The bit
    /// means *some* extent may be compressed, not that the layout is
    /// unreadable, so gating on it made an entire filesystem
    /// unmountable because one file happened to compress well.
    pub const SUPPORTED: u64 = MIXED_BACKREF
        | DEFAULT_SUBVOL
        | MIXED_GROUPS
        | COMPRESS_LZO
        | COMPRESS_ZSTD
        | BIG_METADATA
        | EXTENDED_IREF
        | SKINNY_METADATA
        | NO_HOLES
        | METADATA_UUID
        | RAID1C34;
}

/// `compat_ro_flags` bits: features an implementation may ignore while
/// READING and must not ignore while WRITING.
///
/// A read-only mount accepts any of them. [`Filesystem::mount_rw`] refuses
/// a volume with a bit outside [`compat_ro::SUPPORTED`] -- this driver
/// writes, and a writer that ignores a structure the bit announces leaves
/// it describing a filesystem that no longer exists (#72). See
/// [`refuse_unmaintained_compat_ro`].
///
/// [`Filesystem::mount_rw`]: crate::fs::Filesystem::mount_rw
pub mod compat_ro {
    /// A free-space tree (space_cache v2) is present.
    pub const FREE_SPACE_TREE: u64 = 1 << 0;
    /// The free-space tree is up to date.
    pub const FREE_SPACE_TREE_VALID: u64 = 1 << 1;
    /// fs-verity metadata may be present.
    pub const VERITY: u64 = 1 << 2;
    /// Block group items live in their own tree.
    pub const BLOCK_GROUP_TREE: u64 = 1 << 3;

    /// Every `compat_ro` feature this driver MAINTAINS when it writes.
    ///
    /// The free-space tree is updated by every transaction beside the
    /// extent tree it complements, and its validity bit is cleared when a
    /// commit could not, which is the state the format defines for that.
    /// Not `VERITY`, whose Merkle tree a data write invalidates, and not
    /// `BLOCK_GROUP_TREE`, whose block group items this driver looks for
    /// in the extent tree and would not find.
    pub const SUPPORTED: u64 = FREE_SPACE_TREE | FREE_SPACE_TREE_VALID;
}

/// Refuse to WRITE a volume carrying a `compat_ro` feature this driver
/// does not maintain (#72).
///
/// Reading such a volume is sound, which is what the bit's class says, so
/// this is checked at `mount_rw` and not at parse.
pub fn refuse_unmaintained_compat_ro(compat_ro_flags: u64) -> Result<()> {
    let mut unmaintained = compat_ro_flags & !compat_ro::SUPPORTED;
    if unmaintained == 0 {
        return Ok(());
    }
    let mut named = Vec::new();
    for (bit, name) in [
        (compat_ro::VERITY, "fs-verity"),
        (compat_ro::BLOCK_GROUP_TREE, "the block group tree"),
    ] {
        if unmaintained & bit != 0 {
            named.push(name.to_string());
            unmaintained &= !bit;
        }
    }
    if unmaintained != 0 {
        named.push(format!("unknown bits {unmaintained:#018x}"));
    }
    Err(Error::UnsupportedFeature(format!(
        "this volume uses read-only-compatible features this driver does not maintain \
         ({}), so it can be read but not written",
        named.join(", ")
    )))
}

#[cfg(test)]
mod compat_ro_tests {
    use super::*;

    /// The bit values, against `include/uapi/linux/btrfs.h` on
    /// torvalds/linux master.
    #[test]
    fn the_compat_ro_bits_are_upstreams() {
        assert_eq!(compat_ro::FREE_SPACE_TREE, 1 << 0);
        assert_eq!(compat_ro::FREE_SPACE_TREE_VALID, 1 << 1);
        assert_eq!(compat_ro::VERITY, 1 << 2);
        assert_eq!(compat_ro::BLOCK_GROUP_TREE, 1 << 3);
    }

    #[test]
    fn a_volume_with_only_maintained_features_may_be_written() {
        for flags in [
            0,
            compat_ro::FREE_SPACE_TREE,
            compat_ro::FREE_SPACE_TREE | compat_ro::FREE_SPACE_TREE_VALID,
        ] {
            refuse_unmaintained_compat_ro(flags)
                .unwrap_or_else(|e| panic!("{flags:#x} must be writable: {e:?}"));
        }
    }

    #[test]
    fn a_volume_with_an_unmaintained_feature_is_refused_by_name() {
        for (flags, needle) in [
            (compat_ro::VERITY, "fs-verity"),
            (compat_ro::BLOCK_GROUP_TREE, "block group tree"),
            (1 << 40, "0x0000010000000000"),
            (
                compat_ro::FREE_SPACE_TREE | compat_ro::BLOCK_GROUP_TREE | (1 << 7),
                "0x0000000000000080",
            ),
        ] {
            match refuse_unmaintained_compat_ro(flags) {
                Err(Error::UnsupportedFeature(m)) => {
                    assert!(m.contains(needle), "{flags:#x}: {m}");
                    assert!(m.contains("can be read but not written"), "{flags:#x}: {m}");
                }
                other => panic!("{flags:#x} gave {other:?}"),
            }
        }
    }
}

/// The hash algorithm protecting this volume's metadata, from
/// `csum_type` at [`offsets::CSUM_TYPE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumType {
    /// CRC32C (Castagnoli), 4-byte digest. The `mkfs.btrfs` default.
    Crc32c,
    /// XXH64 with seed 0, 8-byte digest.
    XxHash64,
    /// SHA-256, 32-byte digest.
    Sha256,
    /// BLAKE2b truncated to 256 bits, 32-byte digest.
    Blake2b256,
}

impl ChecksumType {
    /// Decode the on-disk `csum_type` selector.
    pub fn from_raw(raw: u16) -> Result<Self> {
        match raw {
            0 => Ok(ChecksumType::Crc32c),
            1 => Ok(ChecksumType::XxHash64),
            2 => Ok(ChecksumType::Sha256),
            3 => Ok(ChecksumType::Blake2b256),
            other => Err(Error::UnsupportedChecksum(other)),
        }
    }

    /// The on-disk selector value.
    pub fn to_raw(self) -> u16 {
        match self {
            ChecksumType::Crc32c => 0,
            ChecksumType::XxHash64 => 1,
            ChecksumType::Sha256 => 2,
            ChecksumType::Blake2b256 => 3,
        }
    }

    /// How many of the 32 checksum bytes this algorithm actually fills.
    /// The remainder is zero-padding and is not compared.
    pub fn digest_len(self) -> usize {
        match self {
            ChecksumType::Crc32c => 4,
            ChecksumType::XxHash64 => 8,
            ChecksumType::Sha256 | ChecksumType::Blake2b256 => 32,
        }
    }

    /// Compute the digest of `data` into a zero-padded 32-byte field, in
    /// the exact on-disk representation.
    ///
    /// The CRC32C and XXH64 digests are stored **little-endian**, like
    /// every other integer in the format. That is worth spelling out
    /// because the sister XFS driver stores its CRC little-endian while
    /// storing everything else big-endian, and the instinct to treat a
    /// checksum as "the odd one out" does not transfer here.
    pub fn digest(self, data: &[u8]) -> [u8; CSUM_SIZE] {
        let mut out = [0u8; CSUM_SIZE];
        match self {
            ChecksumType::Crc32c => {
                out[..4].copy_from_slice(&crc32c::crc32c(data).to_le_bytes());
            }
            ChecksumType::XxHash64 => {
                let h = twox_hash::XxHash64::oneshot(0, data);
                out[..8].copy_from_slice(&h.to_le_bytes());
            }
            ChecksumType::Sha256 => {
                use sha2::Digest;
                let h = sha2::Sha256::digest(data);
                out.copy_from_slice(&h);
            }
            ChecksumType::Blake2b256 => {
                let h = blake2b_simd::Params::new().hash_length(32).hash(data);
                out.copy_from_slice(h.as_bytes());
            }
        }
        out
    }

    /// Whether `stored` matches the digest of `data`. Only the first
    /// [`Self::digest_len`] bytes participate, matching what the kernel
    /// compares.
    pub fn verify(self, data: &[u8], stored: &[u8]) -> bool {
        let n = self.digest_len();
        if stored.len() < n {
            return false;
        }
        self.digest(data)[..n] == stored[..n]
    }
}

/// The `struct btrfs_dev_item` embedded in the superblock, describing the
/// device the superblock was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevItem {
    /// Device id — the value chunk stripes reference.
    pub devid: u64,
    /// Device capacity in bytes.
    pub total_bytes: u64,
    /// Bytes of this device claimed by chunks.
    pub bytes_used: u64,
    /// Optimal I/O alignment.
    pub io_align: u32,
    /// Optimal I/O width.
    pub io_width: u32,
    /// Minimal I/O size.
    pub sector_size: u32,
    /// Device type flags.
    pub dev_type: u64,
    /// Generation.
    pub generation: u64,
    /// Bytes at the front of the device that are not used by the
    /// filesystem.
    pub start_offset: u64,
    /// Device group.
    pub dev_group: u32,
    /// Seek speed hint.
    pub seek_speed: u8,
    /// Bandwidth hint.
    pub bandwidth: u8,
    /// This device's UUID.
    pub uuid: [u8; UUID_SIZE],
    /// The owning filesystem's UUID. Holds `metadata_uuid` rather than
    /// `fsid` when the `METADATA_UUID` incompatible feature is set.
    pub fsid: [u8; UUID_SIZE],
}

impl DevItem {
    /// Parse a device item from `buf`, which must be at least
    /// [`DEV_ITEM_SIZE`] bytes.
    pub fn parse(buf: &[u8]) -> Result<Self> {
        use dev_item_offsets as o;
        if buf.len() < DEV_ITEM_SIZE {
            return Err(Error::BadSuperblock(format!(
                "dev_item needs {DEV_ITEM_SIZE} bytes, got {}",
                buf.len()
            )));
        }
        Ok(DevItem {
            devid: le64(buf, o::DEVID),
            total_bytes: le64(buf, o::TOTAL_BYTES),
            bytes_used: le64(buf, o::BYTES_USED),
            io_align: le32(buf, o::IO_ALIGN),
            io_width: le32(buf, o::IO_WIDTH),
            sector_size: le32(buf, o::SECTOR_SIZE),
            dev_type: le64(buf, o::TYPE),
            generation: le64(buf, o::GENERATION),
            start_offset: le64(buf, o::START_OFFSET),
            dev_group: le32(buf, o::DEV_GROUP),
            seek_speed: buf[o::SEEK_SPEED],
            bandwidth: buf[o::BANDWIDTH],
            uuid: uuid_at(buf, o::UUID),
            fsid: uuid_at(buf, o::FSID),
        })
    }
}

/// A parsed Btrfs superblock.
///
/// Field names follow the on-disk names so a reader can match this
/// against the format documentation without a translation table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Superblock {
    /// The raw 32-byte checksum field, as stored.
    pub csum: [u8; CSUM_SIZE],
    /// Filesystem UUID.
    pub fsid: [u8; UUID_SIZE],
    /// Physical byte offset this copy belongs at.
    pub bytenr: u64,
    /// Superblock flags — see [`super_flags`].
    pub flags: u64,
    /// Transaction id that wrote this superblock.
    pub generation: u64,
    /// Logical address of the root tree's root node.
    pub root: u64,
    /// Logical address of the chunk tree's root node.
    pub chunk_root: u64,
    /// Logical address of the log tree's root node; 0 when clean.
    pub log_root: u64,
    /// Legacy `log_root_transid`, retained for completeness.
    pub log_root_transid: u64,
    /// Total size of the filesystem across all devices.
    pub total_bytes: u64,
    /// Bytes currently allocated.
    pub bytes_used: u64,
    /// Objectid of the root directory (conventionally 6).
    pub root_dir_objectid: u64,
    /// Number of devices in the filesystem.
    pub num_devices: u64,
    /// Sector size in bytes.
    pub sectorsize: u32,
    /// Metadata node size in bytes.
    pub nodesize: u32,
    /// Legacy `leafsize`. Modern kernels ignore it entirely; it is parsed
    /// but deliberately not validated, because old images are free to
    /// disagree with `nodesize` here and rejecting them would be wrong.
    pub leafsize: u32,
    /// Stripe size in bytes.
    pub stripesize: u32,
    /// Valid byte count within the system chunk array.
    pub sys_chunk_array_size: u32,
    /// Transaction id of the chunk tree root.
    pub chunk_root_generation: u64,
    /// Compatible feature mask.
    pub compat_flags: u64,
    /// Read-only-compatible feature mask.
    pub compat_ro_flags: u64,
    /// Incompatible feature mask.
    pub incompat_flags: u64,
    /// Metadata checksum algorithm.
    pub csum_type: ChecksumType,
    /// Height of the root tree.
    pub root_level: u8,
    /// Height of the chunk tree.
    pub chunk_root_level: u8,
    /// Height of the log tree.
    pub log_root_level: u8,
    /// Device item for the device this superblock was read from.
    pub dev_item: DevItem,
    /// Volume label, trailing NULs trimmed.
    pub label: String,
    /// Free-space-cache generation.
    pub cache_generation: u64,
    /// UUID tree generation.
    pub uuid_tree_generation: u64,
    /// UUID stamped into tree node headers when `METADATA_UUID` is set.
    /// Use [`Self::node_uuid`] rather than reading this directly.
    pub metadata_uuid: [u8; UUID_SIZE],
    /// Number of global roots (extent-tree-v2 era).
    pub nr_global_roots: u64,
    /// The valid prefix of the embedded system chunk array, copied out.
    /// Feed this to [`crate::chunk::ChunkMap::from_sys_chunk_array`].
    pub sys_chunk_array: Vec<u8>,
}

/// Reading fixed-width little-endian fields out of a byte slice.
///
/// Btrfs is little-endian throughout. There is deliberately no
/// big-endian half: a big-endian read in this crate would be a bug, and
/// a helper for it would make the bug spellable.
///
/// These live here rather than in an `endian` module of their own
/// because the superblock is the first thing anything parses, and every
/// other module already imports from it. `block_group` and `fs` each
/// used to declare their own copies — three definitions of `le64` in
/// one crate, byte-identical.
///
/// # Panics
///
/// If the slice is too short. Every caller has already length-checked
/// its buffer against the structure it is parsing; that check belongs
/// where the structure's size is known, not repeated per field. A panic
/// here means a caller skipped it, which is a bug in this crate rather
/// than bad input.
///
/// Read a little-endian `u16` at `off`.
#[inline]
pub(crate) fn le16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(b[off..off + 2].try_into().expect("2 bytes"))
}

/// Read a little-endian `u32` at `off`.
#[inline]
pub(crate) fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().expect("4 bytes"))
}

/// Read a little-endian `u64` at `off`.
#[inline]
pub(crate) fn le64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().expect("8 bytes"))
}

/// Copy a 16-byte UUID out at `off`.
#[inline]
pub(crate) fn uuid_at(b: &[u8], off: usize) -> [u8; UUID_SIZE] {
    b[off..off + UUID_SIZE].try_into().expect("16 bytes")
}

impl Superblock {
    /// Parse and validate a superblock from the first [`SUPER_INFO_SIZE`]
    /// bytes of `buf`.
    ///
    /// The full 4 KiB must be present even though the parsed fields stop
    /// well short of it: the checksum covers the whole structure.
    ///
    /// # Errors
    ///
    /// [`Error::NotBtrfs`] if the magic does not match,
    /// [`Error::UnsupportedChecksum`] if `csum_type` names an algorithm
    /// this driver has no implementation for,
    /// [`Error::ChecksumMismatch`] if the stored digest disagrees,
    /// [`Error::UnsupportedFeature`] if an incompatible feature bit this
    /// driver does not implement is set, and [`Error::BadSuperblock`] if
    /// a geometry field is out of range or internally inconsistent.
    pub fn parse(buf: &[u8]) -> Result<Self> {
        if buf.len() < SUPER_INFO_SIZE {
            return Err(Error::BadSuperblock(format!(
                "need {SUPER_INFO_SIZE} bytes, got {}",
                buf.len()
            )));
        }

        let magic: [u8; 8] = buf[offsets::MAGIC..offsets::MAGIC + 8]
            .try_into()
            .expect("8 bytes");
        if magic != BTRFS_MAGIC {
            return Err(Error::NotBtrfs { magic });
        }

        // csum_type has to be trusted just far enough to pick an
        // algorithm, before the checksum that protects it can be
        // verified. That is unavoidable, so it is range-checked first and
        // an unknown value is rejected rather than used.
        let csum_type = ChecksumType::from_raw(le16(buf, offsets::CSUM_TYPE))?;

        // The checksum covers 0x20..0x1000 — the whole superblock minus
        // the checksum field. Not "the fields we parse", not "up to
        // sys_chunk_array". A shorter span passes on fixtures this crate
        // builds itself and fails on every filesystem the kernel made.
        let csum: [u8; CSUM_SIZE] = buf[offsets::CSUM..offsets::CSUM + CSUM_SIZE]
            .try_into()
            .expect("32 bytes");
        let covered = &buf[CSUM_SIZE..SUPER_INFO_SIZE];
        if !csum_type.verify(covered, &csum) {
            return Err(Error::ChecksumMismatch {
                what: "superblock",
                offset: le64(buf, offsets::BYTENR),
            });
        }

        let incompat_flags = le64(buf, offsets::INCOMPAT_FLAGS);
        let unknown = incompat_flags & !incompat::SUPPORTED;
        if unknown != 0 {
            return Err(Error::UnsupportedFeature(format!(
                "incompatible feature bits {unknown:#018x} not implemented"
            )));
        }

        let sys_len = le32(buf, offsets::SYS_CHUNK_ARRAY_SIZE) as usize;
        if sys_len > SYS_CHUNK_ARRAY_SIZE {
            return Err(Error::BadSuperblock(format!(
                "sys_chunk_array_size {sys_len} exceeds the {SYS_CHUNK_ARRAY_SIZE}-byte array"
            )));
        }
        let sys_chunk_array =
            buf[offsets::SYS_CHUNK_ARRAY..offsets::SYS_CHUNK_ARRAY + sys_len].to_vec();

        let label_raw = &buf[offsets::LABEL..offsets::LABEL + LABEL_SIZE];
        let label_end = label_raw.iter().position(|&b| b == 0).unwrap_or(LABEL_SIZE);
        let label = String::from_utf8_lossy(&label_raw[..label_end]).into_owned();

        let fsid = uuid_at(buf, offsets::FSID);
        let sb = Superblock {
            csum,
            fsid,
            bytenr: le64(buf, offsets::BYTENR),
            flags: le64(buf, offsets::FLAGS),
            generation: le64(buf, offsets::GENERATION),
            root: le64(buf, offsets::ROOT),
            chunk_root: le64(buf, offsets::CHUNK_ROOT),
            log_root: le64(buf, offsets::LOG_ROOT),
            log_root_transid: le64(buf, offsets::LOG_ROOT_TRANSID),
            total_bytes: le64(buf, offsets::TOTAL_BYTES),
            bytes_used: le64(buf, offsets::BYTES_USED),
            root_dir_objectid: le64(buf, offsets::ROOT_DIR_OBJECTID),
            num_devices: le64(buf, offsets::NUM_DEVICES),
            sectorsize: le32(buf, offsets::SECTORSIZE),
            nodesize: le32(buf, offsets::NODESIZE),
            leafsize: le32(buf, offsets::LEAFSIZE),
            stripesize: le32(buf, offsets::STRIPESIZE),
            sys_chunk_array_size: sys_len as u32,
            chunk_root_generation: le64(buf, offsets::CHUNK_ROOT_GENERATION),
            compat_flags: le64(buf, offsets::COMPAT_FLAGS),
            compat_ro_flags: le64(buf, offsets::COMPAT_RO_FLAGS),
            incompat_flags,
            csum_type,
            root_level: buf[offsets::ROOT_LEVEL],
            chunk_root_level: buf[offsets::CHUNK_ROOT_LEVEL],
            log_root_level: buf[offsets::LOG_ROOT_LEVEL],
            dev_item: DevItem::parse(&buf[offsets::DEV_ITEM..offsets::DEV_ITEM + DEV_ITEM_SIZE])?,
            label,
            cache_generation: le64(buf, offsets::CACHE_GENERATION),
            uuid_tree_generation: le64(buf, offsets::UUID_TREE_GENERATION),
            // The on-disk field is written only when the METADATA_UUID
            // feature is enabled; without it the field is all zeros and
            // the metadata UUID *is* the fsid. Verified against real
            // media: mkfs.btrfs leaves 0x23b zeroed, while the reference
            // tooling reports metadata_uuid identical to fsid. Returning
            // the raw zeros here would make every tree node fail its
            // identity check on an ordinary filesystem.
            metadata_uuid: if le64(buf, offsets::INCOMPAT_FLAGS) & incompat::METADATA_UUID != 0 {
                uuid_at(buf, offsets::METADATA_UUID)
            } else {
                fsid
            },
            nr_global_roots: le64(buf, offsets::NR_GLOBAL_ROOTS),
            sys_chunk_array,
        };

        sb.validate()?;
        Ok(sb)
    }

    /// Parse a superblock that was read from `offset`, and additionally
    /// require the copy to agree that it belongs there.
    ///
    /// Btrfs writes its own physical offset into `bytenr`, which makes a
    /// misdirected read detectable independently of the checksum — a
    /// stale-but-intact superblock from a previous filesystem at a
    /// different offset has a perfectly good digest.
    pub fn parse_at(buf: &[u8], offset: u64) -> Result<Self> {
        let sb = Self::parse(buf)?;
        if sb.bytenr != offset {
            return Err(Error::BlockIdentityMismatch {
                what: "superblock",
                expected: offset,
                found: sb.bytenr,
            });
        }
        Ok(sb)
    }

    /// Structural sanity checks.
    ///
    /// Btrfs has fewer redundant `log2` companions than XFS does, so the
    /// checks here lean on cross-field agreement instead: `bytenr` must
    /// name a real mirror offset, tree roots must be sector-aligned,
    /// `dev_item.fsid` must agree with the volume's own identity, and the
    /// size fields must bracket each other. Each of those fails loudly if
    /// a field is read at the wrong offset.
    fn validate(&self) -> Result<()> {
        self.validate_geometry()?;
        self.validate_tree_roots()?;
        self.validate_sys_chunk_array()?;
        self.validate_device_identity()?;
        Ok(())
    }

    /// Sizes, counts and capacities: the numbers describing the device
    /// itself, independent of any tree it holds.
    fn validate_geometry(&self) -> Result<()> {
        let bad = |m: String| Err(Error::BadSuperblock(m));
        if !SUPER_OFFSETS.contains(&self.bytenr) {
            return bad(format!(
                "bytenr {:#x} is not one of the superblock mirror offsets {SUPER_OFFSETS:#x?}",
                self.bytenr
            ));
        }
        if !self.sectorsize.is_power_of_two()
            || !(MIN_SECTORSIZE..=MAX_METADATA_BLOCKSIZE).contains(&self.sectorsize)
        {
            return bad(format!(
                "sectorsize {} is not a power of two in [{MIN_SECTORSIZE}, {MAX_METADATA_BLOCKSIZE}]",
                self.sectorsize
            ));
        }
        if !self.nodesize.is_power_of_two()
            || self.nodesize < self.sectorsize
            || self.nodesize > MAX_METADATA_BLOCKSIZE
        {
            return bad(format!(
                "nodesize {} is not a power of two in [sectorsize {}, {MAX_METADATA_BLOCKSIZE}]",
                self.nodesize, self.sectorsize
            ));
        }
        // Modern kernels require stripesize == sectorsize exactly. Old
        // mkfs.btrfs wrote a hard-coded 4096 here, which agrees with the
        // rule for every volume that also uses 4 KiB sectors — i.e. for
        // essentially all of them. Flagged for cross-validation: if a
        // real 16 KiB-sector image ever trips this, relax it to a
        // power-of-two check.
        if self.stripesize != self.sectorsize {
            return bad(format!(
                "stripesize {} disagrees with sectorsize {}",
                self.stripesize, self.sectorsize
            ));
        }
        if self.num_devices == 0 {
            return bad("num_devices is zero".into());
        }
        if self.total_bytes == 0 {
            return bad("total_bytes is zero".into());
        }
        if !self.total_bytes.is_multiple_of(u64::from(self.sectorsize)) {
            return bad(format!(
                "total_bytes {} is not a multiple of sectorsize {}",
                self.total_bytes, self.sectorsize
            ));
        }
        if self.bytes_used > self.total_bytes {
            return bad(format!(
                "bytes_used {} exceeds total_bytes {}",
                self.bytes_used, self.total_bytes
            ));
        }
        Ok(())
    }

    /// The three tree roots the superblock points at: their depth must be
    /// within the format's maximum tree height, their addresses must be
    /// sector-aligned, and the two that are mandatory must be present.
    fn validate_tree_roots(&self) -> Result<()> {
        let bad = |m: String| Err(Error::BadSuperblock(m));
        for (name, level) in [
            ("root_level", self.root_level),
            ("chunk_root_level", self.chunk_root_level),
            ("log_root_level", self.log_root_level),
        ] {
            if level >= MAX_LEVEL {
                return bad(format!(
                    "{name} {level} is at or above the maximum tree height {MAX_LEVEL}"
                ));
            }
        }
        for (name, addr) in [
            ("root", self.root),
            ("chunk_root", self.chunk_root),
            ("log_root", self.log_root),
        ] {
            if !addr.is_multiple_of(u64::from(self.sectorsize)) {
                return bad(format!(
                    "{name} {addr:#x} is not sectorsize-aligned ({})",
                    self.sectorsize
                ));
            }
        }
        if self.chunk_root == 0 {
            return bad("chunk_root is zero — nothing can be mapped without it".into());
        }
        if self.root == 0 {
            return bad("root tree address is zero".into());
        }
        Ok(())
    }

    /// The bootstrap chunk mapping embedded in the superblock. Without at
    /// least one usable entry the chunk tree cannot be located, and
    /// nothing else on the volume can be read.
    fn validate_sys_chunk_array(&self) -> Result<()> {
        let bad = |m: String| Err(Error::BadSuperblock(m));
        // The system chunk array must be able to hold at least one
        // (key, chunk item, stripe) triple, otherwise the chunk tree is
        // unreachable. 17 (key) + 48 (chunk header) + 32 (one stripe)
        // = 97. The reference implementation makes the same check, its
        // chunk structure carrying one embedded stripe.
        const MIN_SYS_CHUNK_ARRAY: u32 = 17 + 48 + 32;
        if self.sys_chunk_array_size < MIN_SYS_CHUNK_ARRAY {
            return bad(format!(
                "sys_chunk_array_size {} cannot hold a single chunk mapping (need {MIN_SYS_CHUNK_ARRAY})",
                self.sys_chunk_array_size
            ));
        }
        Ok(())
    }

    /// The device's own record of which filesystem it belongs to.
    fn validate_device_identity(&self) -> Result<()> {
        let bad = |m: String| Err(Error::BadSuperblock(m));
        // Every device carries the identity of the filesystem it belongs
        // to. Skipped while an fsid change is in flight or when this is a
        // seed device, because in those states the two legitimately
        // disagree.
        let identity_in_flux = self.flags
            & (super_flags::CHANGING_FSID | super_flags::CHANGING_FSID_V2 | super_flags::SEEDING)
            != 0;
        if !identity_in_flux && self.dev_item.fsid != self.node_uuid() {
            return bad("dev_item.fsid does not match the volume's own UUID".into());
        }
        Ok(())
    }

    /// The UUID stamped into tree node headers.
    ///
    /// Equals [`Self::fsid`] unless the `METADATA_UUID` incompatible
    /// feature is set, in which case the volume's visible UUID and the
    /// one written into metadata have been deliberately decoupled (that
    /// is the whole point of the feature — it lets `fsid` be changed
    /// without rewriting every tree block).
    pub fn node_uuid(&self) -> [u8; UUID_SIZE] {
        if self.incompat_flags & incompat::METADATA_UUID != 0 {
            self.metadata_uuid
        } else {
            self.fsid
        }
    }

    /// Whether the log tree holds changes not yet folded into the main
    /// trees. Reading the committed trees remains safe; the data is just
    /// slightly behind the last `fsync`.
    ///
    /// Gates: every mount refuses such a volume with `Error::DirtyLog`.
    pub fn has_dirty_log(&self) -> bool {
        self.log_root != 0
    }

    /// Whether this device is a read-only seed for another filesystem.
    ///
    /// Gates: `Filesystem::mount_rw` refuses a seed; it mounts read-only.
    pub fn is_seeding(&self) -> bool {
        self.flags & super_flags::SEEDING != 0
    }

    /// Whether the volume was flagged as having hit an error.
    ///
    /// An accessor for a consumer; the driver does not branch on it. This
    /// crate has nowhere to surface a warning, so whether to tell a user
    /// before a write is the caller's decision.
    pub fn has_error_flag(&self) -> bool {
        self.flags & super_flags::ERROR != 0
    }

    /// Whether this image came from a metadata-only dump tool. Such an
    /// image has no data extents and cannot be mounted.
    ///
    /// Gates: every mount refuses one.
    pub fn is_metadump(&self) -> bool {
        self.flags & (super_flags::METADUMP | super_flags::METADUMP_V2) != 0
    }

    /// Whether file holes are implicit rather than recorded as extents.
    ///
    /// An accessor; the reader handles both layouts without branching.
    pub fn has_no_holes(&self) -> bool {
        self.incompat_flags & incompat::NO_HOLES != 0
    }

    /// Whether metadata back-references use the compact "skinny" form.
    ///
    /// An accessor; `extent_write` reads the incompat bit itself.
    pub fn has_skinny_metadata(&self) -> bool {
        self.incompat_flags & incompat::SKINNY_METADATA != 0
    }

    /// Whether data and metadata share block groups.
    ///
    /// An accessor; the reader handles both layouts without branching, as
    /// the `mixed` fixture shows.
    pub fn has_mixed_groups(&self) -> bool {
        self.incompat_flags & incompat::MIXED_GROUPS != 0
    }

    /// Whether a free-space tree (space_cache v2) is present and valid.
    ///
    /// An accessor; the write path consults the compat_ro bits directly.
    pub fn has_free_space_tree(&self) -> bool {
        self.compat_ro_flags & (compat_ro::FREE_SPACE_TREE | compat_ro::FREE_SPACE_TREE_VALID)
            == (compat_ro::FREE_SPACE_TREE | compat_ro::FREE_SPACE_TREE_VALID)
    }

    /// Whether block group items live in their own tree.
    ///
    /// Gates, through the compat_ro bit: `mount_rw` refuses such a volume
    /// (`refuse_unmaintained_compat_ro`, #72), because the write path
    /// looks for block group items in the extent tree. Reading does not
    /// use block groups.
    pub fn has_block_group_tree(&self) -> bool {
        self.compat_ro_flags & compat_ro::BLOCK_GROUP_TREE != 0
    }
}

/// The superblock a mount should use: the valid copy with the highest
/// `generation`, and which copy that was (0 is the primary).
///
/// EVERY COPY IS READ (#90). Every read used copy 0 alone, so a volume
/// whose primary at 64 KiB was damaged -- by a stray `dd`, a partitioner,
/// or a commit of this driver's interrupted after copy 0 -- could not be
/// mounted, though the commit path writes all three. Reading every copy
/// rather than stopping at the first that parses is what lets a torn
/// commit's newer copy win over an older one; ties go to the lower copy.
///
/// A copy past the end of the device is ABSENT, not damaged: a volume
/// under 256 GiB has no copy 2, and one under 64 MiB has only copy 0.
///
/// # Errors
///
/// When no copy is valid, the primary's own error -- the one a caller
/// would have seen before.
pub fn read_superblock(dev: &dyn fs_core::BlockRead) -> Result<(Superblock, usize)> {
    let size = dev.size_bytes();
    let mut best: Option<(Superblock, usize)> = None;
    let mut primary_error = None;
    for (copy, &offset) in SUPER_OFFSETS.iter().enumerate() {
        if offset.saturating_add(SUPER_INFO_SIZE as u64) > size {
            continue;
        }
        let mut buf = vec![0u8; SUPER_INFO_SIZE];
        let parsed = dev
            .read_at(offset, &mut buf)
            .map_err(Error::from)
            .and_then(|()| Superblock::parse_at(&buf, offset));
        match parsed {
            Ok(sb) => {
                if best
                    .as_ref()
                    .is_none_or(|(b, _)| sb.generation > b.generation)
                {
                    best = Some((sb, copy));
                }
            }
            Err(e) if copy == 0 => primary_error = Some(e),
            Err(_) => {}
        }
    }
    best.ok_or_else(|| {
        primary_error.unwrap_or_else(|| {
            Error::BadSuperblock("the device is too small to hold a superblock".to_string())
        })
    })
}

#[cfg(test)]
pub(crate) mod tests {
    //! Unit tests over hand-built superblocks.
    //!
    //! **These are necessary but not sufficient.** Every fixture below is
    //! assembled by this test module using the same offset and byte-order
    //! constants the parser uses, so a misreading of the on-disk format
    //! would be baked into both sides and the tests would pass anyway.
    //! What they *can* catch is a regression against the format as this
    //! crate currently understands it, plus arithmetic and bounds errors.
    //! What they cannot catch is the crate understanding the format
    //! wrongly in the first place. Only a fixture produced by a real
    //! `mkfs.btrfs` can settle that, which is what the cross-validation
    //! harness exists for.

    use super::*;

    pub(crate) const TEST_FSID: [u8; UUID_SIZE] = [0xA1; UUID_SIZE];
    const TEST_DEV_UUID: [u8; UUID_SIZE] = [0xB2; UUID_SIZE];

    /// Build a syntactically valid superblock: 4 KiB sectors, 16 KiB
    /// nodes, single device, CRC32C.
    pub(crate) fn sb_bytes() -> Vec<u8> {
        let mut b = vec![0u8; SUPER_INFO_SIZE];
        b[offsets::MAGIC..offsets::MAGIC + 8].copy_from_slice(&BTRFS_MAGIC);
        b[offsets::FSID..offsets::FSID + UUID_SIZE].copy_from_slice(&TEST_FSID);
        put64(&mut b, offsets::BYTENR, SUPER_INFO_OFFSET);
        put64(&mut b, offsets::GENERATION, 7);
        put64(&mut b, offsets::ROOT, 0x2000_0000);
        put64(&mut b, offsets::CHUNK_ROOT, 0x100_0000);
        put64(&mut b, offsets::TOTAL_BYTES, 1 << 30);
        put64(&mut b, offsets::BYTES_USED, 1 << 20);
        put64(&mut b, offsets::ROOT_DIR_OBJECTID, 6);
        put64(&mut b, offsets::NUM_DEVICES, 1);
        put32(&mut b, offsets::SECTORSIZE, 4096);
        put32(&mut b, offsets::NODESIZE, 16384);
        put32(&mut b, offsets::LEAFSIZE, 16384);
        put32(&mut b, offsets::STRIPESIZE, 4096);
        put32(&mut b, offsets::SYS_CHUNK_ARRAY_SIZE, 97);
        put64(&mut b, offsets::CHUNK_ROOT_GENERATION, 7);
        put64(
            &mut b,
            offsets::INCOMPAT_FLAGS,
            incompat::MIXED_BACKREF | incompat::BIG_METADATA | incompat::SKINNY_METADATA,
        );
        put16(&mut b, offsets::CSUM_TYPE, 0);
        b[offsets::ROOT_LEVEL] = 1;
        b[offsets::CHUNK_ROOT_LEVEL] = 0;
        b[offsets::LOG_ROOT_LEVEL] = 0;

        // dev_item
        let d = offsets::DEV_ITEM;
        put64(&mut b, d + dev_item_offsets::DEVID, 1);
        put64(&mut b, d + dev_item_offsets::TOTAL_BYTES, 1 << 30);
        put64(&mut b, d + dev_item_offsets::BYTES_USED, 1 << 20);
        put32(&mut b, d + dev_item_offsets::IO_ALIGN, 4096);
        put32(&mut b, d + dev_item_offsets::IO_WIDTH, 4096);
        put32(&mut b, d + dev_item_offsets::SECTOR_SIZE, 4096);
        b[d + dev_item_offsets::UUID..d + dev_item_offsets::UUID + UUID_SIZE]
            .copy_from_slice(&TEST_DEV_UUID);
        b[d + dev_item_offsets::FSID..d + dev_item_offsets::FSID + UUID_SIZE]
            .copy_from_slice(&TEST_FSID);

        b[offsets::LABEL..offsets::LABEL + 5].copy_from_slice(b"disks");

        reseal(&mut b);
        b
    }

    /// Recompute the checksum after mutating a fixture.
    pub(crate) fn reseal(b: &mut [u8]) {
        let t = ChecksumType::from_raw(le16(b, offsets::CSUM_TYPE)).unwrap_or(ChecksumType::Crc32c);
        let digest = t.digest(&b[CSUM_SIZE..SUPER_INFO_SIZE]);
        b[..CSUM_SIZE].copy_from_slice(&digest);
    }

    fn put16(b: &mut [u8], off: usize, v: u16) {
        b[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(b: &mut [u8], off: usize, v: u32) {
        b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn put64(b: &mut [u8], off: usize, v: u64) {
        b[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn magic_bytes_and_u64_form_agree() {
        assert_eq!(u64::from_le_bytes(BTRFS_MAGIC), BTRFS_MAGIC_LE64);
        assert_eq!(&BTRFS_MAGIC, b"_BHRfS_M");
    }

    #[test]
    fn offsets_are_contiguous_and_end_where_the_format_says() {
        // Each of these is "previous offset + previous size". If any one
        // is mistyped the chain stops closing on 0x32b, which is the one
        // property of this table an independent source pins down.
        assert_eq!(offsets::CSUM + CSUM_SIZE, offsets::FSID);
        assert_eq!(offsets::FSID + UUID_SIZE, offsets::BYTENR);
        assert_eq!(offsets::BYTENR + 8, offsets::FLAGS);
        assert_eq!(offsets::FLAGS + 8, offsets::MAGIC);
        assert_eq!(offsets::MAGIC + 8, offsets::GENERATION);
        assert_eq!(offsets::GENERATION + 8, offsets::ROOT);
        assert_eq!(offsets::ROOT + 8, offsets::CHUNK_ROOT);
        assert_eq!(offsets::CHUNK_ROOT + 8, offsets::LOG_ROOT);
        assert_eq!(offsets::LOG_ROOT + 8, offsets::LOG_ROOT_TRANSID);
        assert_eq!(offsets::LOG_ROOT_TRANSID + 8, offsets::TOTAL_BYTES);
        assert_eq!(offsets::TOTAL_BYTES + 8, offsets::BYTES_USED);
        assert_eq!(offsets::BYTES_USED + 8, offsets::ROOT_DIR_OBJECTID);
        assert_eq!(offsets::ROOT_DIR_OBJECTID + 8, offsets::NUM_DEVICES);
        assert_eq!(offsets::NUM_DEVICES + 8, offsets::SECTORSIZE);
        assert_eq!(offsets::SECTORSIZE + 4, offsets::NODESIZE);
        assert_eq!(offsets::NODESIZE + 4, offsets::LEAFSIZE);
        assert_eq!(offsets::LEAFSIZE + 4, offsets::STRIPESIZE);
        assert_eq!(offsets::STRIPESIZE + 4, offsets::SYS_CHUNK_ARRAY_SIZE);
        assert_eq!(
            offsets::SYS_CHUNK_ARRAY_SIZE + 4,
            offsets::CHUNK_ROOT_GENERATION
        );
        assert_eq!(offsets::CHUNK_ROOT_GENERATION + 8, offsets::COMPAT_FLAGS);
        assert_eq!(offsets::COMPAT_FLAGS + 8, offsets::COMPAT_RO_FLAGS);
        assert_eq!(offsets::COMPAT_RO_FLAGS + 8, offsets::INCOMPAT_FLAGS);
        assert_eq!(offsets::INCOMPAT_FLAGS + 8, offsets::CSUM_TYPE);
        assert_eq!(offsets::CSUM_TYPE + 2, offsets::ROOT_LEVEL);
        assert_eq!(offsets::ROOT_LEVEL + 1, offsets::CHUNK_ROOT_LEVEL);
        assert_eq!(offsets::CHUNK_ROOT_LEVEL + 1, offsets::LOG_ROOT_LEVEL);
        assert_eq!(offsets::LOG_ROOT_LEVEL + 1, offsets::DEV_ITEM);
        assert_eq!(offsets::DEV_ITEM + DEV_ITEM_SIZE, offsets::LABEL);
        assert_eq!(offsets::LABEL + LABEL_SIZE, offsets::CACHE_GENERATION);
        assert_eq!(offsets::CACHE_GENERATION + 8, offsets::UUID_TREE_GENERATION);
        assert_eq!(offsets::UUID_TREE_GENERATION + 8, offsets::METADATA_UUID);
        assert_eq!(offsets::METADATA_UUID + UUID_SIZE, offsets::NR_GLOBAL_ROOTS);
        // 216 reserved bytes separate nr_global_roots from the array.
        assert_eq!(offsets::NR_GLOBAL_ROOTS + 8 + 216, offsets::SYS_CHUNK_ARRAY);
        assert_eq!(
            offsets::SYS_CHUNK_ARRAY + SYS_CHUNK_ARRAY_SIZE,
            offsets::SUPER_ROOTS
        );
        // 4 backup roots of 168 bytes each, then 565 padding bytes.
        assert_eq!(offsets::SUPER_ROOTS + 4 * 168 + 565, SUPER_INFO_SIZE);
    }

    #[test]
    fn dev_item_offsets_close_on_its_size() {
        assert_eq!(dev_item_offsets::FSID + UUID_SIZE, DEV_ITEM_SIZE);
    }

    #[test]
    fn parses_a_valid_superblock() {
        let sb = Superblock::parse(&sb_bytes()).unwrap();
        assert_eq!(sb.bytenr, SUPER_INFO_OFFSET);
        assert_eq!(sb.sectorsize, 4096);
        assert_eq!(sb.nodesize, 16384);
        assert_eq!(sb.num_devices, 1);
        assert_eq!(sb.generation, 7);
        assert_eq!(sb.root, 0x2000_0000);
        assert_eq!(sb.chunk_root, 0x100_0000);
        assert_eq!(sb.total_bytes, 1 << 30);
        assert_eq!(sb.csum_type, ChecksumType::Crc32c);
        assert_eq!(sb.label, "disks");
        assert_eq!(sb.fsid, TEST_FSID);
        assert_eq!(sb.dev_item.devid, 1);
        assert_eq!(sb.dev_item.uuid, TEST_DEV_UUID);
        assert!(!sb.has_dirty_log());
    }

    #[test]
    fn parse_at_accepts_the_matching_offset() {
        let sb = Superblock::parse_at(&sb_bytes(), SUPER_INFO_OFFSET).unwrap();
        assert_eq!(sb.bytenr, SUPER_INFO_OFFSET);
    }

    #[test]
    fn parse_at_rejects_a_misdirected_read() {
        assert!(matches!(
            Superblock::parse_at(&sb_bytes(), SUPER_OFFSETS[1]),
            Err(Error::BlockIdentityMismatch { .. })
        ));
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut b = sb_bytes();
        b[offsets::MAGIC..offsets::MAGIC + 8].copy_from_slice(b"NOTBTRFS");
        reseal(&mut b);
        assert!(matches!(Superblock::parse(&b), Err(Error::NotBtrfs { .. })));
    }

    /// The decisive byte-order regression test. Btrfs's magic is an ASCII
    /// string; a reader that reversed it — or that read the field as a
    /// big-endian integer and re-serialised it — must reject the volume
    /// rather than quietly accepting its own mirror image.
    #[test]
    fn rejects_byte_reversed_magic() {
        let mut b = sb_bytes();
        let mut rev = BTRFS_MAGIC;
        rev.reverse();
        b[offsets::MAGIC..offsets::MAGIC + 8].copy_from_slice(&rev);
        reseal(&mut b);
        assert!(matches!(Superblock::parse(&b), Err(Error::NotBtrfs { .. })));
    }

    #[test]
    fn rejects_short_buffer() {
        assert!(matches!(
            Superblock::parse(&[0u8; 512]),
            Err(Error::BadSuperblock(_))
        ));
        // Even one byte short of the full structure: the checksum needs
        // all 4096 bytes.
        assert!(matches!(
            Superblock::parse(&vec![0u8; SUPER_INFO_SIZE - 1]),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_a_corrupt_checksum() {
        let mut b = sb_bytes();
        b[offsets::GENERATION] ^= 0xFF;
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::ChecksumMismatch { .. })
        ));
    }

    /// The checksum must cover the tail of the superblock, not merely the
    /// fields the parser reads. Flipping a byte in the padding past the
    /// backup roots has to invalidate it.
    #[test]
    fn checksum_covers_the_whole_structure_not_just_parsed_fields() {
        let mut b = sb_bytes();
        b[SUPER_INFO_SIZE - 1] ^= 0xFF;
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::ChecksumMismatch { .. })
        ));
    }

    /// Only the first `digest_len` bytes of the 32-byte field mean
    /// anything for a narrow hash; the kernel ignores the rest and so
    /// must we.
    #[test]
    fn crc32c_ignores_the_unused_tail_of_the_csum_field() {
        let mut b = sb_bytes();
        b[8] = 0xEE;
        b[31] = 0xEE;
        assert!(Superblock::parse(&b).is_ok());
    }

    #[test]
    fn parses_every_supported_checksum_type() {
        for (raw, want) in [
            (0u16, ChecksumType::Crc32c),
            (1, ChecksumType::XxHash64),
            (2, ChecksumType::Sha256),
            (3, ChecksumType::Blake2b256),
        ] {
            let mut b = sb_bytes();
            put16(&mut b, offsets::CSUM_TYPE, raw);
            reseal(&mut b);
            let sb = Superblock::parse(&b).unwrap_or_else(|e| panic!("csum_type {raw}: {e}"));
            assert_eq!(sb.csum_type, want);
            assert_eq!(sb.csum_type.to_raw(), raw);
        }
    }

    #[test]
    fn a_checksum_from_the_wrong_algorithm_is_rejected() {
        // Seal with CRC32C, then claim to be SHA-256.
        let mut b = sb_bytes();
        put16(&mut b, offsets::CSUM_TYPE, 2);
        // Deliberately do NOT reseal: the stored digest is a CRC32C over
        // stale bytes.
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn rejects_unknown_checksum_type() {
        let mut b = sb_bytes();
        put16(&mut b, offsets::CSUM_TYPE, 9);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::UnsupportedChecksum(9))
        ));
    }

    #[test]
    fn rejects_unknown_incompat_bit() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::INCOMPAT_FLAGS, 1 << 40);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::UnsupportedFeature(_))
        ));
    }

    /// Compression and RAID5/6 are known features that this driver
    /// deliberately does not claim. They must be refused, not ignored.
    #[test]
    fn rejects_known_but_unimplemented_incompat_features() {
        for bit in [
            incompat::RAID56,
            incompat::ZONED,
            incompat::EXTENT_TREE_V2,
            incompat::RAID_STRIPE_TREE,
            incompat::SIMPLE_QUOTA,
            incompat::REMAP_TREE,
        ] {
            let mut b = sb_bytes();
            put64(
                &mut b,
                offsets::INCOMPAT_FLAGS,
                incompat::MIXED_BACKREF | bit,
            );
            reseal(&mut b);
            assert!(
                matches!(Superblock::parse(&b), Err(Error::UnsupportedFeature(_))),
                "incompat bit {bit:#x} should have been refused"
            );
        }
    }

    #[test]
    fn accepts_every_supported_incompat_bit_together() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::INCOMPAT_FLAGS, incompat::SUPPORTED);
        // METADATA_UUID is in the supported set, so dev_item.fsid now has
        // to match metadata_uuid rather than fsid.
        b[offsets::METADATA_UUID..offsets::METADATA_UUID + UUID_SIZE].copy_from_slice(&TEST_FSID);
        reseal(&mut b);
        assert!(Superblock::parse(&b).is_ok());
    }

    /// Read-only-compatible bits never block a read-only mount, however
    /// unfamiliar they are.
    #[test]
    fn unknown_compat_ro_bits_are_tolerated() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::COMPAT_RO_FLAGS, 1 << 60);
        reseal(&mut b);
        assert!(Superblock::parse(&b).is_ok());
    }

    #[test]
    fn reports_free_space_tree_and_block_group_tree() {
        let mut b = sb_bytes();
        put64(
            &mut b,
            offsets::COMPAT_RO_FLAGS,
            compat_ro::FREE_SPACE_TREE
                | compat_ro::FREE_SPACE_TREE_VALID
                | compat_ro::BLOCK_GROUP_TREE,
        );
        reseal(&mut b);
        let sb = Superblock::parse(&b).unwrap();
        assert!(sb.has_free_space_tree());
        assert!(sb.has_block_group_tree());
    }

    #[test]
    fn rejects_bytenr_that_is_not_a_mirror_offset() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::BYTENR, 0x1234);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn accepts_each_mirror_offset() {
        for off in SUPER_OFFSETS {
            let mut b = sb_bytes();
            put64(&mut b, offsets::BYTENR, off);
            reseal(&mut b);
            assert_eq!(Superblock::parse_at(&b, off).unwrap().bytenr, off);
        }
    }

    #[test]
    fn rejects_bad_sectorsize() {
        for bad in [0u32, 512, 3000, 1 << 17] {
            let mut b = sb_bytes();
            put32(&mut b, offsets::SECTORSIZE, bad);
            put32(&mut b, offsets::STRIPESIZE, bad);
            reseal(&mut b);
            assert!(
                matches!(Superblock::parse(&b), Err(Error::BadSuperblock(_))),
                "sectorsize {bad} should have been refused"
            );
        }
    }

    #[test]
    fn rejects_nodesize_below_sectorsize() {
        let mut b = sb_bytes();
        put32(&mut b, offsets::NODESIZE, 2048);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_nodesize_above_the_metadata_maximum() {
        let mut b = sb_bytes();
        put32(&mut b, offsets::NODESIZE, 1 << 17);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_stripesize_disagreeing_with_sectorsize() {
        let mut b = sb_bytes();
        put32(&mut b, offsets::STRIPESIZE, 8192);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_zero_devices() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::NUM_DEVICES, 0);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_zero_or_misaligned_total_bytes() {
        for bad in [0u64, (1 << 30) + 1] {
            let mut b = sb_bytes();
            put64(&mut b, offsets::TOTAL_BYTES, bad);
            reseal(&mut b);
            assert!(
                matches!(Superblock::parse(&b), Err(Error::BadSuperblock(_))),
                "total_bytes {bad} should have been refused"
            );
        }
    }

    #[test]
    fn rejects_bytes_used_exceeding_total() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::BYTES_USED, (1u64 << 30) + 4096);
        put64(&mut b, offsets::TOTAL_BYTES, 1 << 30);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_impossible_tree_heights() {
        for off in [
            offsets::ROOT_LEVEL,
            offsets::CHUNK_ROOT_LEVEL,
            offsets::LOG_ROOT_LEVEL,
        ] {
            let mut b = sb_bytes();
            b[off] = MAX_LEVEL;
            reseal(&mut b);
            assert!(
                matches!(Superblock::parse(&b), Err(Error::BadSuperblock(_))),
                "level field at {off:#x} should have been refused"
            );
        }
    }

    #[test]
    fn rejects_unaligned_tree_roots() {
        for off in [offsets::ROOT, offsets::CHUNK_ROOT, offsets::LOG_ROOT] {
            let mut b = sb_bytes();
            put64(&mut b, off, 0x2000_0001);
            reseal(&mut b);
            assert!(
                matches!(Superblock::parse(&b), Err(Error::BadSuperblock(_))),
                "unaligned address at {off:#x} should have been refused"
            );
        }
    }

    #[test]
    fn rejects_zero_chunk_root() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::CHUNK_ROOT, 0);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn rejects_sys_chunk_array_size_out_of_range() {
        for bad in [0u32, 96, (SYS_CHUNK_ARRAY_SIZE + 1) as u32] {
            let mut b = sb_bytes();
            put32(&mut b, offsets::SYS_CHUNK_ARRAY_SIZE, bad);
            reseal(&mut b);
            assert!(
                matches!(Superblock::parse(&b), Err(Error::BadSuperblock(_))),
                "sys_chunk_array_size {bad} should have been refused"
            );
        }
    }

    #[test]
    fn copies_out_exactly_the_valid_prefix_of_the_sys_chunk_array() {
        let mut b = sb_bytes();
        put32(&mut b, offsets::SYS_CHUNK_ARRAY_SIZE, 200);
        for (i, slot) in b[offsets::SYS_CHUNK_ARRAY..offsets::SYS_CHUNK_ARRAY + 300]
            .iter_mut()
            .enumerate()
        {
            *slot = (i % 251) as u8;
        }
        reseal(&mut b);
        let sb = Superblock::parse(&b).unwrap();
        assert_eq!(sb.sys_chunk_array.len(), 200);
        assert_eq!(sb.sys_chunk_array[0], 0);
        assert_eq!(sb.sys_chunk_array[199], 199);
    }

    #[test]
    fn rejects_dev_item_fsid_mismatch() {
        let mut b = sb_bytes();
        let d = offsets::DEV_ITEM + dev_item_offsets::FSID;
        b[d..d + UUID_SIZE].copy_from_slice(&[0xCC; UUID_SIZE]);
        reseal(&mut b);
        assert!(matches!(
            Superblock::parse(&b),
            Err(Error::BadSuperblock(_))
        ));
    }

    /// A seed device legitimately carries a different fsid in its device
    /// item, so the identity check has to stand down.
    #[test]
    fn tolerates_dev_item_fsid_mismatch_while_identity_is_in_flux() {
        for flag in [
            super_flags::SEEDING,
            super_flags::CHANGING_FSID,
            super_flags::CHANGING_FSID_V2,
        ] {
            let mut b = sb_bytes();
            let d = offsets::DEV_ITEM + dev_item_offsets::FSID;
            b[d..d + UUID_SIZE].copy_from_slice(&[0xCC; UUID_SIZE]);
            put64(&mut b, offsets::FLAGS, flag);
            reseal(&mut b);
            assert!(
                Superblock::parse(&b).is_ok(),
                "flag {flag:#x} should have suspended the fsid identity check"
            );
        }
    }

    #[test]
    fn node_uuid_follows_the_metadata_uuid_feature() {
        let plain = Superblock::parse(&sb_bytes()).unwrap();
        assert_eq!(plain.node_uuid(), plain.fsid);

        let mut b = sb_bytes();
        put64(
            &mut b,
            offsets::INCOMPAT_FLAGS,
            incompat::MIXED_BACKREF | incompat::METADATA_UUID,
        );
        let meta = [0x5A; UUID_SIZE];
        b[offsets::METADATA_UUID..offsets::METADATA_UUID + UUID_SIZE].copy_from_slice(&meta);
        // dev_item.fsid tracks the metadata UUID once the feature is on.
        let d = offsets::DEV_ITEM + dev_item_offsets::FSID;
        b[d..d + UUID_SIZE].copy_from_slice(&meta);
        reseal(&mut b);
        let sb = Superblock::parse(&b).unwrap();
        assert_eq!(sb.node_uuid(), meta);
        assert_ne!(sb.node_uuid(), sb.fsid);
    }

    #[test]
    fn reports_a_dirty_log() {
        let mut b = sb_bytes();
        put64(&mut b, offsets::LOG_ROOT, 0x3000_0000);
        reseal(&mut b);
        let sb = Superblock::parse(&b).unwrap();
        assert!(sb.has_dirty_log());
    }

    #[test]
    fn reports_flag_bits() {
        let mut b = sb_bytes();
        put64(
            &mut b,
            offsets::FLAGS,
            super_flags::ERROR | super_flags::METADUMP_V2,
        );
        reseal(&mut b);
        let sb = Superblock::parse(&b).unwrap();
        assert!(sb.has_error_flag());
        assert!(sb.is_metadump());
        assert!(!sb.is_seeding());
    }

    #[test]
    fn label_is_trimmed_at_the_first_nul_and_a_full_label_survives() {
        let sb = Superblock::parse(&sb_bytes()).unwrap();
        assert_eq!(sb.label, "disks");

        let mut b = sb_bytes();
        b[offsets::LABEL..offsets::LABEL + LABEL_SIZE].copy_from_slice(&[b'x'; LABEL_SIZE]);
        reseal(&mut b);
        assert_eq!(Superblock::parse(&b).unwrap().label.len(), LABEL_SIZE);
    }

    #[test]
    fn checksum_digest_lengths_match_the_algorithms() {
        assert_eq!(ChecksumType::Crc32c.digest_len(), 4);
        assert_eq!(ChecksumType::XxHash64.digest_len(), 8);
        assert_eq!(ChecksumType::Sha256.digest_len(), 32);
        assert_eq!(ChecksumType::Blake2b256.digest_len(), 32);
        for t in [
            ChecksumType::Crc32c,
            ChecksumType::XxHash64,
            ChecksumType::Sha256,
            ChecksumType::Blake2b256,
        ] {
            let d = t.digest(b"btrfs");
            assert_eq!(d.len(), CSUM_SIZE);
            assert!(d[t.digest_len()..].iter().all(|&b| b == 0));
            assert!(t.verify(b"btrfs", &d));
            assert!(!t.verify(b"btrfz", &d));
            assert!(!t.verify(b"btrfs", &d[..t.digest_len() - 1]));
        }
    }

    /// CRC32C is stored little-endian, like every other integer in the
    /// format. Pin the byte order so a future refactor cannot quietly
    /// swap it.
    #[test]
    fn crc32c_digest_is_stored_little_endian() {
        let raw = crc32c::crc32c(b"btrfs");
        let d = ChecksumType::Crc32c.digest(b"btrfs");
        assert_eq!(&d[..4], &raw.to_le_bytes());
        assert_ne!(&d[..4], &raw.to_be_bytes());
    }
}

#[cfg(test)]
mod endian_tests {
    use super::{le16, le32, le64};

    /// Byte order, against literal bytes.
    ///
    /// Asserting it against `from_le_bytes` would restate the
    /// implementation. These are the numbers a hex dump shows — and the
    /// property that matters, since a byte-order slip is invisible to a
    /// round trip through this crate: reader and writer would agree
    /// with each other and disagree with the kernel.
    #[test]
    fn the_low_byte_comes_first() {
        let b = [0x01u8, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(le16(&b, 0), 0x0201);
        assert_eq!(le32(&b, 0), 0x0403_0201);
        assert_eq!(le64(&b, 0), 0x0807_0605_0403_0201);
    }

    /// The offset counts bytes, not units of the width being read.
    #[test]
    fn the_offset_counts_bytes() {
        let b = [0x00u8, 0xAA, 0xBB, 0x00];
        assert_eq!(le16(&b, 1), 0xBBAA);
    }

    #[test]
    fn the_widest_values_survive() {
        let b = [0xFFu8; 8];
        assert_eq!(le16(&b, 0), u16::MAX);
        assert_eq!(le32(&b, 0), u32::MAX);
        assert_eq!(le64(&b, 0), u64::MAX);
    }
}
