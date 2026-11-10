//! Chunk items and the logical-to-physical address map.
//!
//! # Why this module comes first
//!
//! Every pointer in Btrfs — tree roots, tree node children, file extents
//! — is a *logical* address in a single flat 64-bit space that spans the
//! whole filesystem, however many devices it has. Nothing in the volume
//! can be read until that space can be translated to a `(device, byte
//! offset)` pair, and the translation table is itself stored in a tree
//! (the chunk tree) whose root is named by a logical address.
//!
//! Btrfs breaks that circle with the `sys_chunk_array` embedded in the
//! superblock: a plain, unindexed run of `(key, chunk item)` pairs
//! covering every SYSTEM chunk, which between them cover the chunk tree.
//! Load those, and the chunk tree becomes readable; read the chunk tree,
//! and the rest of the filesystem becomes readable. That bootstrap is
//! [`ChunkMap::from_sys_chunk_array`].
//!
//! # Byte order
//!
//! Little-endian throughout, like the rest of the format.
//!
//! # Address arithmetic
//!
//! A chunk maps a contiguous logical range onto one or more *stripes*,
//! each a contiguous byte range on one device. How an offset within the
//! chunk selects a stripe depends on the chunk's RAID profile:
//!
//! - SINGLE — one stripe; the offset passes straight through.
//! - DUP, RAID1, RAID1C3, RAID1C4 — every stripe is a full copy of the
//!   whole chunk, so the offset passes straight through and the stripe
//!   index selects *which mirror* to read. DUP puts its copies on the
//!   same device; the RAID1 family puts them on different ones.
//! - RAID0 — the chunk is cut into `stripe_len` rows dealt round-robin
//!   across the stripes.
//! - RAID10 — RAID0 across `num_stripes / sub_stripes` groups, with each
//!   group mirrored `sub_stripes` ways.
//!
//! RAID5 and RAID6 are deliberately **not** implemented. Their layout
//! involves a rotating parity stripe, and a plausible-looking guess would
//! silently return the wrong bytes rather than fail; [`Error::UnsupportedProfile`]
//! is the honest answer.

use crate::error::{Error, Result};
use crate::superblock::{le16, le32, le64, uuid_at, Superblock, UUID_SIZE};

/// Size of an on-disk `struct btrfs_disk_key`: `u64` objectid, `u8` type,
/// `u64` offset, packed with no padding.
pub const DISK_KEY_SIZE: usize = 17;

/// Size of the fixed part of a `struct btrfs_chunk`, before its stripe
/// array.
pub const CHUNK_HEADER_SIZE: usize = 48;

/// Size of one on-disk `struct btrfs_stripe`.
pub const STRIPE_SIZE: usize = 32;

/// The stripe length every current `mkfs.btrfs` uses (`BTRFS_STRIPE_LEN`).
///
/// The value is read from each chunk item rather than assumed, but it is
/// worth naming: modern kernels treat 64 KiB as a constant and will
/// complain about anything else.
pub const DEFAULT_STRIPE_LEN: u64 = 64 * 1024;

/// Well-known key type numbers.
pub mod key_type {
    /// `BTRFS_EXTENT_ITEM_KEY` — an allocated run. Its key's `offset`
    /// is the run's length in bytes.
    pub const EXTENT_ITEM: u8 = 168;
    /// `BTRFS_METADATA_ITEM_KEY` — an allocated tree block, under the
    /// `SKINNY_METADATA` feature. Its key's `offset` is the block's
    /// LEVEL, not a length: the length is always `nodesize`. Reading it
    /// as a length is the mistake the feature invites, and it produces
    /// extents of length 0, 1 and 2.
    pub const METADATA_ITEM: u8 = 169;
    /// `BTRFS_DEV_ITEM_KEY`.
    pub const DEV_ITEM: u8 = 216;
    /// `BTRFS_CHUNK_ITEM_KEY`.
    pub const CHUNK_ITEM: u8 = 228;
    /// `BTRFS_BLOCK_GROUP_ITEM_KEY` — one allocation region. Filed
    /// under the region's first byte, with its length as the `offset`.
    pub const BLOCK_GROUP_ITEM: u8 = 192;
    /// `BTRFS_FREE_SPACE_INFO_KEY` — how a block group's free space is
    /// recorded, extents or a bitmap.
    pub const FREE_SPACE_INFO: u8 = 198;
    /// `BTRFS_FREE_SPACE_EXTENT_KEY` — one free run. The item has no
    /// body; the key is the whole of it.
    pub const FREE_SPACE_EXTENT: u8 = 199;
    /// `BTRFS_FREE_SPACE_BITMAP_KEY` — free space as one bit per
    /// sector.
    pub const FREE_SPACE_BITMAP: u8 = 200;
}

/// Well-known objectid numbers.
pub mod objectid {
    /// `BTRFS_DEV_ITEMS_OBJECTID` — the objectid device items are filed
    /// under. Shares the numeric value 1 with the root tree's objectid;
    /// they live in different trees, so there is no ambiguity.
    pub const DEV_ITEMS: u64 = 1;
    /// `BTRFS_ROOT_TREE_OBJECTID`.
    pub const ROOT_TREE: u64 = 1;
    /// `BTRFS_EXTENT_TREE_OBJECTID`.
    pub const EXTENT_TREE: u64 = 2;
    /// `BTRFS_CHUNK_TREE_OBJECTID`.
    pub const CHUNK_TREE: u64 = 3;
    /// `BTRFS_DEV_TREE_OBJECTID`.
    pub const DEV_TREE: u64 = 4;
    /// `BTRFS_FS_TREE_OBJECTID`.
    pub const FS_TREE: u64 = 5;
    /// `BTRFS_FREE_SPACE_TREE_OBJECTID` — the tree recording what is
    /// free, as opposed to the extent tree recording what is not.
    pub const FREE_SPACE_TREE: u64 = 10;
    /// `BTRFS_FIRST_CHUNK_TREE_OBJECTID` — the objectid every chunk item
    /// is filed under.
    pub const FIRST_CHUNK_TREE: u64 = 256;
}

/// Chunk / block-group type bits, as stored in a chunk item's `type`
/// field.
pub mod block_group {
    /// Chunk holds file data.
    pub const DATA: u64 = 1 << 0;
    /// Chunk holds system metadata — the chunk tree itself.
    pub const SYSTEM: u64 = 1 << 1;
    /// Chunk holds filesystem metadata.
    pub const METADATA: u64 = 1 << 2;
    /// Striped with no redundancy.
    pub const RAID0: u64 = 1 << 3;
    /// Two-way mirror across devices.
    pub const RAID1: u64 = 1 << 4;
    /// Two copies on the same device.
    pub const DUP: u64 = 1 << 5;
    /// Striped mirrors.
    pub const RAID10: u64 = 1 << 6;
    /// Single-parity stripe set.
    pub const RAID5: u64 = 1 << 7;
    /// Double-parity stripe set.
    pub const RAID6: u64 = 1 << 8;
    /// Three-way mirror across devices.
    pub const RAID1C3: u64 = 1 << 9;
    /// Four-way mirror across devices.
    pub const RAID1C4: u64 = 1 << 10;

    /// Every bit that names a redundancy profile. A chunk may set at most
    /// one of them; setting none means SINGLE.
    pub const PROFILE_MASK: u64 = RAID0 | RAID1 | RAID1C3 | RAID1C4 | RAID5 | RAID6 | DUP | RAID10;

    /// Every bit that names what a chunk holds. A chunk must set at least
    /// one.
    pub const TYPE_MASK: u64 = DATA | SYSTEM | METADATA;
}

/// The redundancy layout of a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkProfile {
    /// One stripe, no redundancy.
    Single,
    /// Two copies on the same device.
    Dup,
    /// Striped across all stripes, no redundancy.
    Raid0,
    /// Two-way mirror.
    Raid1,
    /// Three-way mirror.
    Raid1c3,
    /// Four-way mirror.
    Raid1c4,
    /// Striped mirrors.
    Raid10,
    /// Single-parity — recognised, not translatable.
    Raid5,
    /// Double-parity — recognised, not translatable.
    Raid6,
}

impl ChunkProfile {
    /// Derive the profile from a chunk item's `type` field.
    ///
    /// # Errors
    ///
    /// [`Error::BadChunkItem`] if more than one profile bit is set, or if
    /// the chunk does not say what it holds.
    pub fn from_type(chunk_type: u64) -> Result<Self> {
        if chunk_type & block_group::TYPE_MASK == 0 {
            return Err(Error::BadChunkItem(format!(
                "type {chunk_type:#x} names neither DATA, METADATA nor SYSTEM"
            )));
        }
        let profile = chunk_type & block_group::PROFILE_MASK;
        if profile.count_ones() > 1 {
            return Err(Error::BadChunkItem(format!(
                "type {chunk_type:#x} sets {} profile bits at once",
                profile.count_ones()
            )));
        }
        Ok(match profile {
            0 => ChunkProfile::Single,
            block_group::DUP => ChunkProfile::Dup,
            block_group::RAID0 => ChunkProfile::Raid0,
            block_group::RAID1 => ChunkProfile::Raid1,
            block_group::RAID1C3 => ChunkProfile::Raid1c3,
            block_group::RAID1C4 => ChunkProfile::Raid1c4,
            block_group::RAID10 => ChunkProfile::Raid10,
            block_group::RAID5 => ChunkProfile::Raid5,
            block_group::RAID6 => ChunkProfile::Raid6,
            other => {
                return Err(Error::BadChunkItem(format!(
                    "type {chunk_type:#x} has unrecognised profile bit {other:#x}"
                )))
            }
        })
    }

    /// Human-readable profile name, matching the names `mkfs.btrfs` and
    /// `btrfs filesystem df` use.
    pub fn name(self) -> &'static str {
        match self {
            ChunkProfile::Single => "single",
            ChunkProfile::Dup => "dup",
            ChunkProfile::Raid0 => "raid0",
            ChunkProfile::Raid1 => "raid1",
            ChunkProfile::Raid1c3 => "raid1c3",
            ChunkProfile::Raid1c4 => "raid1c4",
            ChunkProfile::Raid10 => "raid10",
            ChunkProfile::Raid5 => "raid5",
            ChunkProfile::Raid6 => "raid6",
        }
    }

    /// Whether every stripe is a full copy of the chunk, so an offset
    /// within the chunk maps through unchanged and the stripe index picks
    /// a mirror.
    pub fn is_mirrored(self) -> bool {
        matches!(
            self,
            ChunkProfile::Dup | ChunkProfile::Raid1 | ChunkProfile::Raid1c3 | ChunkProfile::Raid1c4
        )
    }

    /// Whether the chunk is cut into rows dealt across its stripes.
    pub fn is_striped(self) -> bool {
        matches!(self, ChunkProfile::Raid0 | ChunkProfile::Raid10)
    }
}

/// A `struct btrfs_disk_key` — the 17-byte sort key every tree item and
/// every `sys_chunk_array` entry is filed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskKey {
    /// What the item is about.
    pub objectid: u64,
    /// What kind of item it is — see [`key_type`].
    pub key_type: u8,
    /// Type-specific qualifier. For a chunk item this is the logical
    /// address the chunk starts at.
    pub offset: u64,
}

impl DiskKey {
    /// Parse a key from the first [`DISK_KEY_SIZE`] bytes of `buf`.
    pub fn parse(buf: &[u8]) -> Result<Self> {
        if buf.len() < DISK_KEY_SIZE {
            return Err(Error::BadChunkItem(format!(
                "key needs {DISK_KEY_SIZE} bytes, got {}",
                buf.len()
            )));
        }
        Ok(DiskKey {
            // The key is a packed 17-byte record with no padding, so
            // `offset` starts at an unaligned byte 9. Reading it as if
            // the struct were aligned would skip three bytes into the
            // wrong field.
            objectid: le64(buf, 0),
            key_type: buf[8],
            // Deliberately unaligned: the on-disk struct is packed, so
            // this `u64` starts on byte 9.
            offset: le64(buf, 9),
        })
    }
}

/// One `struct btrfs_stripe` — a contiguous byte range on one device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stripe {
    /// Which device this stripe lives on.
    pub devid: u64,
    /// Byte offset of the stripe on that device.
    pub offset: u64,
    /// UUID of the device, cross-checked against the device's own item.
    pub dev_uuid: [u8; UUID_SIZE],
}

impl Stripe {
    /// Parse a stripe from the first [`STRIPE_SIZE`] bytes of `buf`.
    pub fn parse(buf: &[u8]) -> Result<Self> {
        if buf.len() < STRIPE_SIZE {
            return Err(Error::BadChunkItem(format!(
                "stripe needs {STRIPE_SIZE} bytes, got {}",
                buf.len()
            )));
        }
        Ok(Stripe {
            devid: le64(buf, 0),
            offset: le64(buf, 8),
            dev_uuid: uuid_at(buf, 16),
        })
    }
}

/// A parsed `struct btrfs_chunk` together with the logical address it was
/// filed under.
///
/// The logical start address is *not* stored in the chunk item itself —
/// it is the `offset` of the key the item is filed under. That split is
/// easy to miss and produces an address map that is wrong by exactly one
/// chunk's worth of offset, so [`Chunk::logical`] is carried explicitly
/// rather than being left implicit in the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// First logical address this chunk covers, from its key.
    pub logical: u64,
    /// Length of the logical range in bytes.
    pub length: u64,
    /// Objectid of the tree that owns this chunk.
    pub owner: u64,
    /// Row size for striped profiles, in bytes.
    pub stripe_len: u64,
    /// Raw `type` field — see [`block_group`].
    pub chunk_type: u64,
    /// Optimal I/O alignment hint.
    pub io_align: u32,
    /// Optimal I/O width hint.
    pub io_width: u32,
    /// Minimal I/O size hint.
    pub sector_size: u32,
    /// Number of stripes that follow.
    pub num_stripes: u16,
    /// Mirrors per striped group, meaningful for RAID10.
    pub sub_stripes: u16,
    /// The stripe array.
    pub stripes: Vec<Stripe>,
}

/// The result of translating one logical address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    /// Device to read from.
    pub devid: u64,
    /// Byte offset on that device.
    pub physical: u64,
    /// How many bytes remain contiguous from [`Self::physical`]. A read
    /// longer than this must be split and re-mapped, because the next
    /// byte of the logical range lives on a different stripe or past the
    /// end of the chunk.
    pub len: u64,
}

impl Chunk {
    /// Parse a chunk item filed under logical address `logical`.
    ///
    /// `buf` must start at the chunk item and may extend past it; the
    /// number of bytes consumed is [`Chunk::encoded_len`].
    ///
    /// # Errors
    ///
    /// [`Error::BadChunkItem`] if the buffer is too short for the stripe
    /// count the header declares, if the geometry is degenerate, or if
    /// the stripe count disagrees with the RAID profile.
    pub fn parse(logical: u64, buf: &[u8]) -> Result<Self> {
        if buf.len() < CHUNK_HEADER_SIZE {
            return Err(Error::BadChunkItem(format!(
                "chunk header needs {CHUNK_HEADER_SIZE} bytes, got {}",
                buf.len()
            )));
        }
        let num_stripes = le16(buf, 0x2c);
        if num_stripes == 0 {
            return Err(Error::BadChunkItem("chunk declares zero stripes".into()));
        }
        let need = CHUNK_HEADER_SIZE + usize::from(num_stripes) * STRIPE_SIZE;
        if buf.len() < need {
            return Err(Error::BadChunkItem(format!(
                "chunk with {num_stripes} stripes needs {need} bytes, got {}",
                buf.len()
            )));
        }

        let mut stripes = Vec::with_capacity(usize::from(num_stripes));
        for raw in buf[CHUNK_HEADER_SIZE..need].chunks_exact(STRIPE_SIZE) {
            stripes.push(Stripe::parse(raw)?);
        }

        let chunk = Chunk {
            logical,
            length: le64(buf, 0x00),
            owner: le64(buf, 0x08),
            stripe_len: le64(buf, 0x10),
            chunk_type: le64(buf, 0x18),
            io_align: le32(buf, 0x20),
            io_width: le32(buf, 0x24),
            sector_size: le32(buf, 0x28),
            num_stripes,
            sub_stripes: le16(buf, 0x2e),
            stripes,
        };
        chunk.validate()?;
        Ok(chunk)
    }

    /// How many bytes this chunk item occupies on disk.
    pub fn encoded_len(&self) -> usize {
        CHUNK_HEADER_SIZE + usize::from(self.num_stripes) * STRIPE_SIZE
    }

    /// One past the last logical address this chunk covers.
    pub fn logical_end(&self) -> u64 {
        self.logical.saturating_add(self.length)
    }

    /// The chunk's redundancy profile.
    pub fn profile(&self) -> Result<ChunkProfile> {
        ChunkProfile::from_type(self.chunk_type)
    }

    /// Whether this chunk holds system metadata (the chunk tree).
    pub fn is_system(&self) -> bool {
        self.chunk_type & block_group::SYSTEM != 0
    }

    /// Whether this chunk holds filesystem metadata.
    pub fn is_metadata(&self) -> bool {
        self.chunk_type & block_group::METADATA != 0
    }

    /// Whether this chunk holds file data.
    pub fn is_data(&self) -> bool {
        self.chunk_type & block_group::DATA != 0
    }

    /// How many independent copies of any given byte this chunk holds.
    /// Mirror indices passed to [`Chunk::map_mirror`] run `0 ..
    /// num_mirrors()`.
    pub fn num_mirrors(&self) -> usize {
        match self.profile() {
            Ok(ChunkProfile::Dup | ChunkProfile::Raid1) => 2,
            Ok(ChunkProfile::Raid1c3) => 3,
            Ok(ChunkProfile::Raid1c4) => 4,
            Ok(ChunkProfile::Raid10) => usize::from(self.sub_stripes),
            _ => 1,
        }
    }

    /// Structural checks that do not need any outside context.
    fn validate(&self) -> Result<()> {
        let bad = |m: String| Err(Error::BadChunkItem(m));

        if self.length == 0 {
            return bad("chunk covers zero logical bytes".into());
        }
        if self.stripe_len == 0 || !self.stripe_len.is_power_of_two() {
            return bad(format!(
                "stripe_len {} is not a non-zero power of two",
                self.stripe_len
            ));
        }
        if self.logical.checked_add(self.length).is_none() {
            return bad(format!(
                "logical range {:#x}+{:#x} overflows the address space",
                self.logical, self.length
            ));
        }

        let profile = self.profile()?;
        let n = self.num_stripes;
        // Stripe counts are fixed by the profile for everything except
        // RAID0 and the parity profiles. Checking them is cheap and
        // catches a stripe array read at the wrong offset, which would
        // otherwise produce a map that is subtly wrong rather than
        // obviously broken.
        let expect_exact = match profile {
            ChunkProfile::Single => Some(1),
            ChunkProfile::Dup | ChunkProfile::Raid1 => Some(2),
            ChunkProfile::Raid1c3 => Some(3),
            ChunkProfile::Raid1c4 => Some(4),
            _ => None,
        };
        if let Some(want) = expect_exact {
            if n != want {
                return bad(format!(
                    "{} chunk has {n} stripes, expected {want}",
                    profile.name()
                ));
            }
        }
        // Same shape as `expect_exact` above, and deliberately so: these
        // profiles constrain the stripe count from below rather than
        // pinning it. Written as a table because the alternative clippy
        // suggests — a `ChunkProfile::Raid0 if n < 2 =>` match guard —
        // makes the arm match only the *failing* case, leaving a valid
        // raid0 to fall through to the catch-all. That reads as though
        // raid0 were unvalidated.
        let expect_at_least = match profile {
            ChunkProfile::Raid0 | ChunkProfile::Raid5 => Some(2),
            ChunkProfile::Raid6 => Some(3),
            _ => None,
        };
        if let Some(least) = expect_at_least {
            if n < least {
                return bad(format!(
                    "{} chunk has {n} stripes, expected at least {least}",
                    profile.name()
                ));
            }
        }
        if profile == ChunkProfile::Raid10 {
            // Modern mkfs always writes sub_stripes = 2 for RAID10,
            // and the kernel's own validator rejects anything else.
            // Flagged for cross-validation: if a real image ever
            // shows a different value, relax this to "non-zero and
            // divides num_stripes".
            if self.sub_stripes != 2 {
                return bad(format!(
                    "raid10 chunk has sub_stripes {}, expected 2",
                    self.sub_stripes
                ));
            }
            if n < 2 || !n.is_multiple_of(self.sub_stripes) {
                return bad(format!(
                    "raid10 chunk has {n} stripes, which is not a positive multiple of sub_stripes {}",
                    self.sub_stripes
                ));
            }
        }
        Ok(())
    }

    /// Checks that need the superblock's geometry.
    ///
    /// Kept separate from `Chunk::validate` so a chunk item can be
    /// parsed and inspected — by a repair tool, say — without a
    /// superblock in hand.
    pub fn validate_geometry(&self, sectorsize: u32) -> Result<()> {
        let s = u64::from(sectorsize);
        if s == 0 {
            return Err(Error::BadChunkItem("sectorsize is zero".into()));
        }
        if !self.logical.is_multiple_of(s) {
            return Err(Error::BadChunkItem(format!(
                "chunk logical start {:#x} is not sectorsize-aligned ({sectorsize})",
                self.logical
            )));
        }
        if !self.length.is_multiple_of(s) {
            return Err(Error::BadChunkItem(format!(
                "chunk length {:#x} is not a multiple of sectorsize {sectorsize}",
                self.length
            )));
        }
        for (i, stripe) in self.stripes.iter().enumerate() {
            if !stripe.offset.is_multiple_of(s) {
                return Err(Error::BadChunkItem(format!(
                    "stripe {i} physical offset {:#x} is not sectorsize-aligned ({sectorsize})",
                    stripe.offset
                )));
            }
        }
        Ok(())
    }

    /// Translate `logical` using the first copy.
    pub fn map(&self, logical: u64) -> Result<Mapping> {
        self.map_mirror(logical, 0)
    }

    /// Translate `logical` using copy `mirror`.
    ///
    /// # Errors
    ///
    /// [`Error::UnmappedLogical`] if the address is outside this chunk,
    /// [`Error::UnsupportedProfile`] for RAID5/RAID6, and
    /// [`Error::BadChunkItem`] if `mirror` names a copy that does not
    /// exist or the stripe arithmetic overflows.
    pub fn map_mirror(&self, logical: u64, mirror: usize) -> Result<Mapping> {
        if logical < self.logical || logical >= self.logical_end() {
            return Err(Error::UnmappedLogical(logical));
        }
        let profile = self.profile()?;
        if matches!(profile, ChunkProfile::Raid5 | ChunkProfile::Raid6) {
            return Err(Error::UnsupportedProfile(format!(
                "{} needs parity-aware placement, which this driver does not implement",
                profile.name()
            )));
        }
        let mirrors = self.num_mirrors();
        if mirror >= mirrors {
            return Err(Error::BadChunkItem(format!(
                "mirror {mirror} requested from a {} chunk with {mirrors} copies",
                profile.name()
            )));
        }

        let offset = logical - self.logical;
        let stripe_nr = offset / self.stripe_len;
        let stripe_offset = offset % self.stripe_len;
        let n = u64::from(self.num_stripes);
        let m = mirror as u64;

        // `index` selects the stripe; `row` is how many full rows of
        // stripe_len precede this one *within that stripe*.
        let (index, row) = match profile {
            ChunkProfile::Single => (0, stripe_nr),
            ChunkProfile::Dup
            | ChunkProfile::Raid1
            | ChunkProfile::Raid1c3
            | ChunkProfile::Raid1c4 => (m, stripe_nr),
            ChunkProfile::Raid0 => (stripe_nr % n, stripe_nr / n),
            ChunkProfile::Raid10 => {
                let sub = u64::from(self.sub_stripes);
                let groups = n / sub;
                ((stripe_nr % groups) * sub + m, stripe_nr / groups)
            }
            ChunkProfile::Raid5 | ChunkProfile::Raid6 => unreachable!("rejected above"),
        };

        let stripe = self
            .stripes
            .get(index as usize)
            .ok_or_else(|| Error::BadChunkItem(format!("stripe index {index} out of range")))?;

        let overflow =
            || Error::BadChunkItem("stripe address arithmetic overflowed 64 bits".into());
        let physical = row
            .checked_mul(self.stripe_len)
            .and_then(|r| r.checked_add(stripe_offset))
            .and_then(|within| stripe.offset.checked_add(within))
            .ok_or_else(overflow)?;

        let to_chunk_end = self.logical_end() - logical;
        let len = if profile.is_striped() {
            (self.stripe_len - stripe_offset).min(to_chunk_end)
        } else {
            to_chunk_end
        };

        Ok(Mapping {
            devid: stripe.devid,
            physical,
            len,
        })
    }
}

/// The logical-to-physical address map: an ordered, non-overlapping set
/// of chunks.
///
/// Bootstrapped from the superblock's `sys_chunk_array` and then extended
/// with the chunk items found in the chunk tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkMap {
    chunks: Vec<Chunk>,
}

/// The chunk an item of the chunk tree or the `sys_chunk_array` describes,
/// or `None` for an item that is not a chunk.
///
/// ONE PLACE FOR BOTH BUILDERS (#85). The superblock array checked the key
/// type, the objectid and the geometry; the chunk-tree walk checked none
/// of them and dropped any item that failed to parse. So a `DEV_ITEM` --
/// which the chunk tree also holds, one per device -- was parsed as a
/// chunk at logical address `devid`, kept out of the map only by its
/// field values; a malformed `CHUNK_ITEM` was discarded and its range
/// surfaced much later as an unmapped read somewhere unrelated; and the
/// geometry check ran on the handful of bootstrap chunks and not on the
/// tree's.
///
/// # Errors
///
/// [`Error::BadChunkItem`] for a `CHUNK_ITEM` under the wrong objectid,
/// one that does not parse, or one whose geometry is not sector-aligned.
pub fn chunk_from_item(key: &DiskKey, data: &[u8], sectorsize: u32) -> Result<Option<Chunk>> {
    if key.key_type != key_type::CHUNK_ITEM {
        return Ok(None);
    }
    if key.objectid != objectid::FIRST_CHUNK_TREE {
        return Err(Error::BadChunkItem(format!(
            "chunk item at logical {:#x} has objectid {}, expected {}",
            key.offset,
            key.objectid,
            objectid::FIRST_CHUNK_TREE
        )));
    }
    // Both refusals name the item, so a mount that fails here points at
    // the chunk rather than at a reason with no address.
    let named = |e| match e {
        Error::BadChunkItem(why) => {
            Error::BadChunkItem(format!("chunk item at logical {:#x}: {why}", key.offset))
        }
        other => other,
    };
    let chunk = Chunk::parse(key.offset, data).map_err(named)?;
    chunk.validate_geometry(sectorsize).map_err(named)?;
    Ok(Some(chunk))
}

impl ChunkMap {
    /// An empty map.
    pub fn new() -> Self {
        ChunkMap::default()
    }

    /// Build the bootstrap map from a superblock's system chunk array.
    ///
    /// This is the one step that breaks the circular dependency between
    /// "read the chunk tree" and "translate a logical address": the array
    /// is a flat list, addressed physically, that covers enough of the
    /// logical space to reach the chunk tree root.
    ///
    /// # Errors
    ///
    /// [`Error::BadChunkItem`] if the array is truncated mid-entry, holds
    /// a key that is not a chunk item, or contains a chunk whose geometry
    /// does not hold up.
    pub fn from_sys_chunk_array(array: &[u8], sectorsize: u32) -> Result<Self> {
        let mut map = ChunkMap::new();
        let mut pos = 0usize;
        while pos < array.len() {
            let key = DiskKey::parse(&array[pos..]).map_err(|_| {
                Error::BadChunkItem(format!(
                    "sys_chunk_array truncated: {} bytes left, need {DISK_KEY_SIZE} for a key",
                    array.len() - pos
                ))
            })?;
            if key.key_type != key_type::CHUNK_ITEM {
                return Err(Error::BadChunkItem(format!(
                    "sys_chunk_array entry at byte {pos} has key type {}, expected {}",
                    key.key_type,
                    key_type::CHUNK_ITEM
                )));
            }
            if key.objectid != objectid::FIRST_CHUNK_TREE {
                return Err(Error::BadChunkItem(format!(
                    "sys_chunk_array entry at byte {pos} has objectid {}, expected {}",
                    key.objectid,
                    objectid::FIRST_CHUNK_TREE
                )));
            }
            pos += DISK_KEY_SIZE;
            let chunk = chunk_from_item(&key, &array[pos..], sectorsize)?
                .expect("the key type was checked above");
            pos += chunk.encoded_len();
            map.insert(chunk)?;
        }
        Ok(map)
    }

    /// Build the bootstrap map straight from a parsed superblock.
    pub fn bootstrap(sb: &Superblock) -> Result<Self> {
        Self::from_sys_chunk_array(&sb.sys_chunk_array, sb.sectorsize)
    }

    /// Add a chunk, keeping the map ordered by logical address.
    ///
    /// # Errors
    ///
    /// [`Error::BadChunkItem`] if the new chunk's logical range overlaps
    /// one already present. Overlapping chunks would make translation
    /// ambiguous, and the ambiguity would show up much later as
    /// unexplained data corruption.
    pub fn insert(&mut self, chunk: Chunk) -> Result<()> {
        let at = self.chunks.partition_point(|c| c.logical < chunk.logical);
        if let Some(prev) = at.checked_sub(1).and_then(|i| self.chunks.get(i)) {
            if prev.logical_end() > chunk.logical {
                return Err(Error::BadChunkItem(format!(
                    "chunk at {:#x} overlaps the chunk at {:#x}",
                    chunk.logical, prev.logical
                )));
            }
        }
        if let Some(next) = self.chunks.get(at) {
            if chunk.logical_end() > next.logical {
                return Err(Error::BadChunkItem(format!(
                    "chunk at {:#x} overlaps the chunk at {:#x}",
                    chunk.logical, next.logical
                )));
            }
        }
        self.chunks.insert(at, chunk);
        Ok(())
    }

    /// Every chunk, ordered by logical address.
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// How many chunks the map holds.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Whether the map is empty. An empty map cannot translate anything.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// The chunk covering `logical`, if any.
    pub fn chunk_for(&self, logical: u64) -> Option<&Chunk> {
        let at = self.chunks.partition_point(|c| c.logical <= logical);
        let candidate = self.chunks.get(at.checked_sub(1)?)?;
        if logical < candidate.logical_end() {
            Some(candidate)
        } else {
            None
        }
    }

    /// Whether any chunk covers `logical`.
    pub fn covers(&self, logical: u64) -> bool {
        self.chunk_for(logical).is_some()
    }

    /// How many copies of `logical` exist.
    ///
    /// One for `Single`, two for `Dup` and `Raid1`, and so on. A WRITE
    /// has to reach every one of them: leaving a mirror holding the
    /// previous contents is not a lesser form of redundancy, it is a
    /// filesystem whose two copies disagree, and which copy a later read
    /// returns is not something the caller chooses.
    pub fn mirrors_at(&self, logical: u64) -> Result<usize> {
        Ok(self
            .chunk_for(logical)
            .ok_or(Error::UnmappedLogical(logical))?
            .num_mirrors())
    }

    /// Translate `logical` using the first copy.
    pub fn map(&self, logical: u64) -> Result<Mapping> {
        self.map_mirror(logical, 0)
    }

    /// Translate `logical` using copy `mirror`.
    pub fn map_mirror(&self, logical: u64, mirror: usize) -> Result<Mapping> {
        self.chunk_for(logical)
            .ok_or(Error::UnmappedLogical(logical))?
            .map_mirror(logical, mirror)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests over hand-built chunk items.
    //!
    //! **Necessary but not sufficient**, for the same reason as the
    //! superblock tests: every fixture here is encoded by this module
    //! using the same offsets the parser decodes with, so a misreading of
    //! `struct btrfs_chunk` would cancel out and go unnoticed. The
    //! address arithmetic is a partial exception — the expected physical
    //! offsets below are worked out by hand from the layout rules rather
    //! than by running the code — but the field offsets underneath them
    //! still need a real `mkfs.btrfs` image to confirm.

    use super::*;

    const DEV_A: u64 = 1;
    const DEV_B: u64 = 2;
    const K64: u64 = 64 * 1024;

    /// Encode a chunk item. `stripes` is `(devid, physical offset)`.
    fn chunk_bytes(
        length: u64,
        stripe_len: u64,
        chunk_type: u64,
        sub_stripes: u16,
        stripes: &[(u64, u64)],
    ) -> Vec<u8> {
        let mut b = vec![0u8; CHUNK_HEADER_SIZE + stripes.len() * STRIPE_SIZE];
        b[0x00..0x08].copy_from_slice(&length.to_le_bytes());
        b[0x08..0x10].copy_from_slice(&objectid::EXTENT_TREE.to_le_bytes());
        b[0x10..0x18].copy_from_slice(&stripe_len.to_le_bytes());
        b[0x18..0x20].copy_from_slice(&chunk_type.to_le_bytes());
        b[0x20..0x24].copy_from_slice(&4096u32.to_le_bytes());
        b[0x24..0x28].copy_from_slice(&4096u32.to_le_bytes());
        b[0x28..0x2c].copy_from_slice(&4096u32.to_le_bytes());
        b[0x2c..0x2e].copy_from_slice(&(stripes.len() as u16).to_le_bytes());
        b[0x2e..0x30].copy_from_slice(&sub_stripes.to_le_bytes());
        for (i, &(devid, offset)) in stripes.iter().enumerate() {
            let at = CHUNK_HEADER_SIZE + i * STRIPE_SIZE;
            b[at..at + 8].copy_from_slice(&devid.to_le_bytes());
            b[at + 8..at + 16].copy_from_slice(&offset.to_le_bytes());
            b[at + 16..at + 32].copy_from_slice(&[(0xD0 + i as u8); UUID_SIZE]);
        }
        b
    }

    /// Encode a `(key, chunk item)` pair the way `sys_chunk_array` does.
    fn sys_entry(logical: u64, chunk: &[u8]) -> Vec<u8> {
        let mut b = Vec::with_capacity(DISK_KEY_SIZE + chunk.len());
        b.extend_from_slice(&objectid::FIRST_CHUNK_TREE.to_le_bytes());
        b.push(key_type::CHUNK_ITEM);
        b.extend_from_slice(&logical.to_le_bytes());
        b.extend_from_slice(chunk);
        b
    }

    fn single_chunk(logical: u64, length: u64, physical: u64) -> Chunk {
        Chunk::parse(
            logical,
            &chunk_bytes(length, K64, block_group::SYSTEM, 0, &[(DEV_A, physical)]),
        )
        .unwrap()
    }

    #[test]
    fn struct_sizes_match_the_on_disk_layout() {
        // btrfs_disk_key is packed: 8 + 1 + 8.
        assert_eq!(DISK_KEY_SIZE, 8 + 1 + 8);
        // btrfs_chunk header: length, owner, stripe_len, type (4 x u64),
        // io_align, io_width, sector_size (3 x u32),
        // num_stripes, sub_stripes (2 x u16).
        assert_eq!(CHUNK_HEADER_SIZE, 4 * 8 + 3 * 4 + 2 * 2);
        // btrfs_stripe: devid, offset, dev_uuid.
        assert_eq!(STRIPE_SIZE, 8 + 8 + UUID_SIZE);
    }

    #[test]
    fn parses_a_disk_key_with_its_unaligned_offset_field() {
        let mut b = vec![0u8; DISK_KEY_SIZE];
        b[0..8].copy_from_slice(&256u64.to_le_bytes());
        b[8] = key_type::CHUNK_ITEM;
        b[9..17].copy_from_slice(&0xDEAD_BEEF_u64.to_le_bytes());
        let k = DiskKey::parse(&b).unwrap();
        assert_eq!(k.objectid, 256);
        assert_eq!(k.key_type, key_type::CHUNK_ITEM);
        assert_eq!(k.offset, 0xDEAD_BEEF);
    }

    #[test]
    fn rejects_a_truncated_disk_key() {
        assert!(matches!(
            DiskKey::parse(&[0u8; 16]),
            Err(Error::BadChunkItem(_))
        ));
    }

    #[test]
    fn parses_a_single_chunk() {
        let c = single_chunk(0x100_0000, 8 * 1024 * 1024, 0x40_0000);
        assert_eq!(c.logical, 0x100_0000);
        assert_eq!(c.length, 8 * 1024 * 1024);
        assert_eq!(c.stripe_len, K64);
        assert_eq!(c.num_stripes, 1);
        assert_eq!(c.profile().unwrap(), ChunkProfile::Single);
        assert!(c.is_system());
        assert!(!c.is_data());
        assert_eq!(c.encoded_len(), CHUNK_HEADER_SIZE + STRIPE_SIZE);
        assert_eq!(c.num_mirrors(), 1);
        assert_eq!(c.stripes[0].devid, DEV_A);
        assert_eq!(c.stripes[0].dev_uuid, [0xD0; UUID_SIZE]);
    }

    /// SINGLE passes the within-chunk offset straight through, and the
    /// contiguous run extends to the end of the chunk rather than to the
    /// end of a stripe.
    #[test]
    fn maps_a_single_chunk() {
        let c = single_chunk(0x100_0000, 8 * 1024 * 1024, 0x40_0000);
        let m = c.map(0x100_0000).unwrap();
        assert_eq!(m.devid, DEV_A);
        assert_eq!(m.physical, 0x40_0000);
        assert_eq!(m.len, 8 * 1024 * 1024);

        let m = c.map(0x100_0000 + 0x1234).unwrap();
        assert_eq!(m.physical, 0x40_0000 + 0x1234);
        assert_eq!(m.len, 8 * 1024 * 1024 - 0x1234);
    }

    #[test]
    fn rejects_addresses_outside_the_chunk() {
        let c = single_chunk(0x100_0000, 0x10_0000, 0x40_0000);
        assert!(matches!(
            c.map(0x100_0000 - 1),
            Err(Error::UnmappedLogical(_))
        ));
        // One past the end is out; the last byte is in.
        assert!(c.map(0x100_0000 + 0x10_0000 - 1).is_ok());
        assert!(matches!(
            c.map(0x100_0000 + 0x10_0000),
            Err(Error::UnmappedLogical(_))
        ));
    }

    /// DUP keeps both copies on one device at different offsets.
    #[test]
    fn maps_both_copies_of_a_dup_chunk() {
        let c = Chunk::parse(
            0,
            &chunk_bytes(
                8 * K64,
                K64,
                block_group::METADATA | block_group::DUP,
                0,
                &[(DEV_A, 0x10_0000), (DEV_A, 0x90_0000)],
            ),
        )
        .unwrap();
        assert_eq!(c.profile().unwrap(), ChunkProfile::Dup);
        assert_eq!(c.num_mirrors(), 2);

        let a = c.map_mirror(0x1000, 0).unwrap();
        let b = c.map_mirror(0x1000, 1).unwrap();
        assert_eq!((a.devid, a.physical), (DEV_A, 0x10_0000 + 0x1000));
        assert_eq!((b.devid, b.physical), (DEV_A, 0x90_0000 + 0x1000));
        assert_eq!(a.len, 8 * K64 - 0x1000);
    }

    /// RAID1 is DUP across two devices.
    #[test]
    fn maps_both_copies_of_a_raid1_chunk() {
        let c = Chunk::parse(
            0,
            &chunk_bytes(
                8 * K64,
                K64,
                block_group::DATA | block_group::RAID1,
                0,
                &[(DEV_A, 0x10_0000), (DEV_B, 0x20_0000)],
            ),
        )
        .unwrap();
        assert_eq!(c.profile().unwrap(), ChunkProfile::Raid1);
        assert_eq!(c.num_mirrors(), 2);
        assert_eq!(c.map_mirror(K64, 0).unwrap().devid, DEV_A);
        assert_eq!(c.map_mirror(K64, 1).unwrap().devid, DEV_B);
        assert_eq!(c.map_mirror(K64, 0).unwrap().physical, 0x10_0000 + K64);
        assert_eq!(c.map_mirror(K64, 1).unwrap().physical, 0x20_0000 + K64);
        assert!(matches!(c.map_mirror(K64, 2), Err(Error::BadChunkItem(_))));
    }

    #[test]
    fn maps_three_and_four_way_mirrors() {
        for (bits, want, count) in [
            (block_group::RAID1C3, ChunkProfile::Raid1c3, 3usize),
            (block_group::RAID1C4, ChunkProfile::Raid1c4, 4),
        ] {
            let stripes: Vec<(u64, u64)> = (0..count as u64)
                .map(|i| (i + 1, 0x10_0000 * (i + 1)))
                .collect();
            let c = Chunk::parse(
                0,
                &chunk_bytes(8 * K64, K64, block_group::METADATA | bits, 0, &stripes),
            )
            .unwrap();
            assert_eq!(c.profile().unwrap(), want);
            assert_eq!(c.num_mirrors(), count);
            for i in 0..count {
                let m = c.map_mirror(0x800, i).unwrap();
                assert_eq!(m.devid, i as u64 + 1);
                assert_eq!(m.physical, 0x10_0000 * (i as u64 + 1) + 0x800);
            }
            assert!(matches!(
                c.map_mirror(0x800, count),
                Err(Error::BadChunkItem(_))
            ));
        }
    }

    /// RAID0 deals `stripe_len` rows round-robin. Four rows across two
    /// devices means logical 0, 1, 2, 3 land on A, B, A, B — with the
    /// third row starting one full stripe further into device A.
    #[test]
    fn maps_a_raid0_chunk_round_robin() {
        let c = Chunk::parse(
            0,
            &chunk_bytes(
                4 * K64,
                K64,
                block_group::DATA | block_group::RAID0,
                0,
                &[(DEV_A, 0x10_0000), (DEV_B, 0x20_0000)],
            ),
        )
        .unwrap();
        assert_eq!(c.profile().unwrap(), ChunkProfile::Raid0);
        assert_eq!(c.num_mirrors(), 1);

        let cases = [
            (0, DEV_A, 0x10_0000),
            (K64, DEV_B, 0x20_0000),
            (2 * K64, DEV_A, 0x10_0000 + K64),
            (3 * K64, DEV_B, 0x20_0000 + K64),
        ];
        for (logical, devid, physical) in cases {
            let m = c.map(logical).unwrap();
            assert_eq!(
                (m.devid, m.physical),
                (devid, physical),
                "logical {logical:#x}"
            );
            assert_eq!(m.len, K64, "a whole stripe should be contiguous");
        }
    }

    /// A read that starts mid-stripe is only contiguous to the end of
    /// that stripe. Getting this wrong reads across a device boundary and
    /// silently returns another device's bytes.
    #[test]
    fn raid0_mapping_length_stops_at_the_stripe_boundary() {
        let c = Chunk::parse(
            0,
            &chunk_bytes(
                4 * K64,
                K64,
                block_group::DATA | block_group::RAID0,
                0,
                &[(DEV_A, 0x10_0000), (DEV_B, 0x20_0000)],
            ),
        )
        .unwrap();
        let m = c.map(2 * K64 + 0x100).unwrap();
        assert_eq!(m.devid, DEV_A);
        assert_eq!(m.physical, 0x10_0000 + K64 + 0x100);
        assert_eq!(m.len, K64 - 0x100);
    }

    /// The last stripe of a chunk is clipped by the chunk end, not by the
    /// stripe length.
    #[test]
    fn striped_mapping_length_is_also_clipped_by_the_chunk_end() {
        let c = Chunk::parse(
            0,
            &chunk_bytes(
                2 * K64 + 0x1000,
                K64,
                block_group::DATA | block_group::RAID0,
                0,
                &[(DEV_A, 0x10_0000), (DEV_B, 0x20_0000)],
            ),
        )
        .unwrap();
        let m = c.map(2 * K64).unwrap();
        assert_eq!(m.devid, DEV_A);
        assert_eq!(m.physical, 0x10_0000 + K64);
        assert_eq!(m.len, 0x1000);
    }

    /// RAID10: two mirrored groups of two. Row 0 lives on stripes 0/1,
    /// row 1 on stripes 2/3, row 2 wraps back to stripes 0/1 one stripe
    /// further in.
    #[test]
    fn maps_a_raid10_chunk() {
        let c = Chunk::parse(
            0,
            &chunk_bytes(
                4 * K64,
                K64,
                block_group::DATA | block_group::RAID10,
                2,
                &[
                    (1, 0x10_0000),
                    (2, 0x20_0000),
                    (3, 0x30_0000),
                    (4, 0x40_0000),
                ],
            ),
        )
        .unwrap();
        assert_eq!(c.profile().unwrap(), ChunkProfile::Raid10);
        assert_eq!(c.num_mirrors(), 2);

        // logical 0 -> group 0, both copies.
        assert_eq!(
            c.map_mirror(0, 0).unwrap(),
            Mapping {
                devid: 1,
                physical: 0x10_0000,
                len: K64
            }
        );
        assert_eq!(
            c.map_mirror(0, 1).unwrap(),
            Mapping {
                devid: 2,
                physical: 0x20_0000,
                len: K64
            }
        );
        // logical 64K -> group 1.
        assert_eq!(
            c.map_mirror(K64, 0).unwrap(),
            Mapping {
                devid: 3,
                physical: 0x30_0000,
                len: K64
            }
        );
        assert_eq!(
            c.map_mirror(K64, 1).unwrap(),
            Mapping {
                devid: 4,
                physical: 0x40_0000,
                len: K64
            }
        );
        // logical 128K -> back to group 0, second row.
        assert_eq!(
            c.map_mirror(2 * K64, 0).unwrap(),
            Mapping {
                devid: 1,
                physical: 0x10_0000 + K64,
                len: K64
            }
        );
        assert!(matches!(c.map_mirror(0, 2), Err(Error::BadChunkItem(_))));
    }

    /// RAID5 and RAID6 must be refused outright rather than mapped as if
    /// they were RAID0. Guessing here returns parity blocks as data.
    #[test]
    fn refuses_to_map_raid5_and_raid6() {
        for (bits, stripes) in [(block_group::RAID5, 3usize), (block_group::RAID6, 4)] {
            let s: Vec<(u64, u64)> = (0..stripes as u64)
                .map(|i| (i + 1, 0x10_0000 * (i + 1)))
                .collect();
            let c = Chunk::parse(
                0,
                &chunk_bytes(4 * K64, K64, block_group::DATA | bits, 0, &s),
            )
            .unwrap();
            assert!(
                matches!(c.map(0), Err(Error::UnsupportedProfile(_))),
                "profile {:?} should be refused",
                c.profile()
            );
        }
    }

    #[test]
    fn rejects_zero_stripes() {
        let mut b = chunk_bytes(K64, K64, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
        b[0x2c..0x2e].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));
    }

    #[test]
    fn rejects_a_truncated_stripe_array() {
        let b = chunk_bytes(K64, K64, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
        assert!(matches!(
            Chunk::parse(0, &b[..b.len() - 1]),
            Err(Error::BadChunkItem(_))
        ));
        assert!(matches!(
            Chunk::parse(0, &b[..CHUNK_HEADER_SIZE - 1]),
            Err(Error::BadChunkItem(_))
        ));
    }

    #[test]
    fn rejects_two_profile_bits_at_once() {
        let b = chunk_bytes(
            8 * K64,
            K64,
            block_group::DATA | block_group::RAID1 | block_group::RAID0,
            0,
            &[(DEV_A, 0), (DEV_B, 0)],
        );
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));
    }

    #[test]
    fn rejects_a_chunk_that_says_nothing_about_what_it_holds() {
        let b = chunk_bytes(8 * K64, K64, 0, 0, &[(DEV_A, 0)]);
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));
    }

    #[test]
    fn rejects_stripe_counts_that_disagree_with_the_profile() {
        // RAID1 with three stripes.
        let b = chunk_bytes(
            8 * K64,
            K64,
            block_group::DATA | block_group::RAID1,
            0,
            &[(1, 0), (2, 0), (3, 0)],
        );
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));

        // SINGLE with two stripes.
        let b = chunk_bytes(8 * K64, K64, block_group::DATA, 0, &[(1, 0), (2, 0)]);
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));

        // RAID0 with one stripe.
        let b = chunk_bytes(
            8 * K64,
            K64,
            block_group::DATA | block_group::RAID0,
            0,
            &[(1, 0)],
        );
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));
    }

    #[test]
    fn rejects_raid10_with_a_bad_sub_stripe_count() {
        // sub_stripes = 0.
        let b = chunk_bytes(
            8 * K64,
            K64,
            block_group::DATA | block_group::RAID10,
            0,
            &[(1, 0), (2, 0), (3, 0), (4, 0)],
        );
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));

        // num_stripes not a multiple of sub_stripes.
        let b = chunk_bytes(
            8 * K64,
            K64,
            block_group::DATA | block_group::RAID10,
            2,
            &[(1, 0), (2, 0), (3, 0)],
        );
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));
    }

    #[test]
    fn rejects_degenerate_geometry() {
        // Zero length.
        let b = chunk_bytes(0, K64, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
        assert!(matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))));

        // Zero and non-power-of-two stripe lengths.
        for bad in [0u64, 3 * 1024] {
            let b = chunk_bytes(K64, bad, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
            assert!(
                matches!(Chunk::parse(0, &b), Err(Error::BadChunkItem(_))),
                "stripe_len {bad} should have been refused"
            );
        }
    }

    #[test]
    fn rejects_a_logical_range_that_overflows() {
        let b = chunk_bytes(K64, K64, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
        assert!(matches!(
            Chunk::parse(u64::MAX - 1024, &b),
            Err(Error::BadChunkItem(_))
        ));
    }

    #[test]
    fn geometry_validation_catches_misalignment() {
        let c = single_chunk(0x100_0000, 0x10_0000, 0x40_0000);
        assert!(c.validate_geometry(4096).is_ok());

        let c = single_chunk(0x100_0000 + 512, 0x10_0000, 0x40_0000);
        assert!(matches!(
            c.validate_geometry(4096),
            Err(Error::BadChunkItem(_))
        ));

        let c = single_chunk(0x100_0000, 0x10_0000 + 512, 0x40_0000);
        assert!(matches!(
            c.validate_geometry(4096),
            Err(Error::BadChunkItem(_))
        ));

        let c = single_chunk(0x100_0000, 0x10_0000, 0x40_0000 + 512);
        assert!(matches!(
            c.validate_geometry(4096),
            Err(Error::BadChunkItem(_))
        ));
    }

    #[test]
    fn bootstraps_a_map_from_a_sys_chunk_array() {
        let mut array = Vec::new();
        array.extend_from_slice(&sys_entry(
            0x100_0000,
            &chunk_bytes(
                0x40_0000,
                K64,
                block_group::SYSTEM,
                0,
                &[(DEV_A, 0x100_0000)],
            ),
        ));
        array.extend_from_slice(&sys_entry(
            0x200_0000,
            &chunk_bytes(
                0x40_0000,
                K64,
                block_group::SYSTEM | block_group::DUP,
                0,
                &[(DEV_A, 0x300_0000), (DEV_A, 0x400_0000)],
            ),
        ));

        let map = ChunkMap::from_sys_chunk_array(&array, 4096).unwrap();
        assert_eq!(map.len(), 2);
        assert!(!map.is_empty());
        assert_eq!(map.chunks()[0].logical, 0x100_0000);
        assert_eq!(map.chunks()[1].profile().unwrap(), ChunkProfile::Dup);

        assert_eq!(map.map(0x100_1000).unwrap().physical, 0x100_1000);
        assert_eq!(map.map(0x200_1000).unwrap().physical, 0x300_1000);
        assert_eq!(map.map_mirror(0x200_1000, 1).unwrap().physical, 0x400_1000);
        assert!(map.covers(0x100_0000));
        assert!(!map.covers(0x180_0000));
        assert!(matches!(
            map.map(0x180_0000),
            Err(Error::UnmappedLogical(_))
        ));
    }

    #[test]
    fn an_empty_sys_chunk_array_yields_an_empty_map() {
        let map = ChunkMap::from_sys_chunk_array(&[], 4096).unwrap();
        assert!(map.is_empty());
        assert!(matches!(map.map(0), Err(Error::UnmappedLogical(0))));
    }

    #[test]
    fn rejects_a_sys_chunk_array_entry_that_is_not_a_chunk_item() {
        let chunk = chunk_bytes(K64, K64, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
        let mut array = sys_entry(0, &chunk);
        array[8] = key_type::DEV_ITEM;
        assert!(matches!(
            ChunkMap::from_sys_chunk_array(&array, 4096),
            Err(Error::BadChunkItem(_))
        ));
    }

    #[test]
    fn rejects_a_sys_chunk_array_entry_filed_under_the_wrong_objectid() {
        let chunk = chunk_bytes(K64, K64, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
        let mut array = sys_entry(0, &chunk);
        array[0..8].copy_from_slice(&7u64.to_le_bytes());
        assert!(matches!(
            ChunkMap::from_sys_chunk_array(&array, 4096),
            Err(Error::BadChunkItem(_))
        ));
    }

    #[test]
    fn rejects_a_truncated_sys_chunk_array() {
        let chunk = chunk_bytes(K64, K64, block_group::SYSTEM, 0, &[(DEV_A, 0)]);
        let array = sys_entry(0, &chunk);
        for cut in [
            1usize,
            DISK_KEY_SIZE - 1,
            DISK_KEY_SIZE + 4,
            array.len() - 1,
        ] {
            assert!(
                matches!(
                    ChunkMap::from_sys_chunk_array(&array[..cut], 4096),
                    Err(Error::BadChunkItem(_))
                ),
                "array truncated to {cut} bytes should have been refused"
            );
        }
    }

    #[test]
    fn rejects_overlapping_chunks() {
        let mut map = ChunkMap::new();
        map.insert(single_chunk(0x100_0000, 0x10_0000, 0)).unwrap();
        // Starts inside the first chunk.
        assert!(matches!(
            map.insert(single_chunk(0x100_8000, 0x10_0000, 0)),
            Err(Error::BadChunkItem(_))
        ));
        // Ends inside the first chunk.
        assert!(matches!(
            map.insert(single_chunk(0x0F0_0000, 0x20_0000, 0)),
            Err(Error::BadChunkItem(_))
        ));
        // Exactly abutting is fine on both sides.
        map.insert(single_chunk(0x110_0000, 0x10_0000, 0)).unwrap();
        map.insert(single_chunk(0x0F0_0000, 0x10_0000, 0)).unwrap();
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn keeps_chunks_ordered_however_they_arrive() {
        let mut map = ChunkMap::new();
        for logical in [0x300_0000, 0x100_0000, 0x200_0000] {
            map.insert(single_chunk(logical, 0x10_0000, logical))
                .unwrap();
        }
        let order: Vec<u64> = map.chunks().iter().map(|c| c.logical).collect();
        assert_eq!(order, vec![0x100_0000, 0x200_0000, 0x300_0000]);
        assert_eq!(map.map(0x200_0000).unwrap().physical, 0x200_0000);
    }

    #[test]
    fn chunk_lookup_handles_the_boundaries() {
        let mut map = ChunkMap::new();
        map.insert(single_chunk(0x100_0000, 0x10_0000, 0)).unwrap();
        map.insert(single_chunk(0x200_0000, 0x10_0000, 0)).unwrap();
        assert!(map.chunk_for(0x0FF_FFFF).is_none());
        assert!(map.chunk_for(0x100_0000).is_some());
        assert!(map.chunk_for(0x10F_FFFF).is_some());
        assert!(map.chunk_for(0x110_0000).is_none());
        assert!(map.chunk_for(0x200_0000).is_some());
        assert!(map.chunk_for(u64::MAX).is_none());
    }

    #[test]
    fn profile_names_and_predicates() {
        assert_eq!(ChunkProfile::Raid1c3.name(), "raid1c3");
        assert!(ChunkProfile::Dup.is_mirrored());
        assert!(ChunkProfile::Raid1c4.is_mirrored());
        assert!(!ChunkProfile::Raid0.is_mirrored());
        assert!(ChunkProfile::Raid10.is_striped());
        assert!(!ChunkProfile::Single.is_striped());
    }

    #[test]
    fn profile_derivation_covers_every_defined_bit() {
        let cases = [
            (0, ChunkProfile::Single),
            (block_group::DUP, ChunkProfile::Dup),
            (block_group::RAID0, ChunkProfile::Raid0),
            (block_group::RAID1, ChunkProfile::Raid1),
            (block_group::RAID1C3, ChunkProfile::Raid1c3),
            (block_group::RAID1C4, ChunkProfile::Raid1c4),
            (block_group::RAID10, ChunkProfile::Raid10),
            (block_group::RAID5, ChunkProfile::Raid5),
            (block_group::RAID6, ChunkProfile::Raid6),
        ];
        for (bits, want) in cases {
            assert_eq!(
                ChunkProfile::from_type(block_group::DATA | bits).unwrap(),
                want
            );
        }
    }

    /// A profile whose stripe count only has a *floor* must reject a
    /// count under it, and accept the floor itself.
    ///
    /// The three profiles share one table, so a bug in the table shows
    /// up as the wrong floor for one of them rather than as no check at
    /// all — which is why each is asserted with its own boundary and its
    /// own message, rather than by looping over "is an error".
    #[test]
    fn a_profile_with_a_minimum_stripe_count_enforces_it() {
        let cases = [
            (block_group::RAID0, 2usize, "raid0"),
            (block_group::RAID5, 2, "raid5"),
            (block_group::RAID6, 3, "raid6"),
        ];
        for (bits, floor, name) in cases {
            let stripes: Vec<(u64, u64)> =
                (0..floor - 1).map(|i| (i as u64 + 1, 0x10_0000)).collect();
            let err = Chunk::parse(
                0,
                &chunk_bytes(4 * K64, K64, block_group::DATA | bits, 0, &stripes),
            )
            .expect_err(&format!(
                "{name} with {} stripes is below its floor of {floor} and must be refused",
                stripes.len()
            ));
            let want = format!(
                "{name} chunk has {} stripes, expected at least {floor}",
                stripes.len()
            );
            assert!(
                err.to_string().contains(&want),
                "{name}: expected a message containing {want:?}, got {err}"
            );

            // And the floor itself is accepted, so the check is a floor
            // and not an off-by-one that refuses valid chunks too.
            let ok: Vec<(u64, u64)> = (0..floor).map(|i| (i as u64 + 1, 0x10_0000)).collect();
            let parsed = Chunk::parse(
                0,
                &chunk_bytes(4 * K64, K64, block_group::DATA | bits, 0, &ok),
            );
            assert!(
                parsed.is_ok(),
                "{name} with exactly {floor} stripes is valid, but parse said {:?}",
                parsed.err()
            );
        }
    }

    /// RAID10 keeps its own arm because it constrains `sub_stripes` as
    /// well as the count, and both rejections must survive.
    #[test]
    fn raid10_checks_sub_stripes_and_the_multiple() {
        let four = [
            (DEV_A, 0x10_0000),
            (DEV_B, 0x20_0000),
            (3, 0x30_0000),
            (4, 0x40_0000),
        ];
        let bits = block_group::DATA | block_group::RAID10;

        let err = Chunk::parse(0, &chunk_bytes(4 * K64, K64, bits, 3, &four))
            .expect_err("sub_stripes 3 is not what mkfs writes and must be refused");
        assert!(
            err.to_string().contains("sub_stripes 3, expected 2"),
            "got {err}"
        );

        let three = &four[..3];
        let err = Chunk::parse(0, &chunk_bytes(4 * K64, K64, bits, 2, three))
            .expect_err("3 stripes is not a multiple of sub_stripes 2");
        assert!(
            err.to_string().contains("not a positive multiple"),
            "got {err}"
        );

        assert!(Chunk::parse(0, &chunk_bytes(4 * K64, K64, bits, 2, &four)).is_ok());
    }
}
