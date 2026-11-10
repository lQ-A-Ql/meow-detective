//! Committing: writing the superblock that makes a transaction real.
//!
//! Everything a Btrfs transaction writes is invisible until the
//! superblock names the new root. Tree blocks can be written, flushed
//! and still count for nothing; the superblock is where a commit happens
//! and the only place it happens.
//!
//! That makes this the most consequential 4 KiB in the filesystem, and
//! the one where being approximately right is worst: a superblock naming
//! a root that was never written, or carrying a stale checksum, does not
//! fail to mount — it mounts against the wrong tree.
//!
//! # What a commit actually changes
//!
//! Measured, not taken from the field table. Six consecutive commits of
//! one real filesystem were captured and compared pairwise. Outside the
//! backup ring, the steady state moves exactly four things:
//!
//! ```text
//! 0x0000  csum                  every commit
//! 0x0048  generation            every commit
//! 0x0050  root                  every commit
//! 0x0233  uuid_tree_generation  every commit, tracking generation
//! ```
//!
//! `bytes_used`, `root_level`, `chunk_root` and `cache_generation` did
//! not move across any of those six, which agrees with the field table's
//! "when usage changes", "when the root tree changes depth" and "when a
//! chunk is allocated" — this workload did none of the three. They are
//! settable here because a commit that does one of those must set them.
//!
//! One field moved once and never again: `incompat_flags`, `0x341` to
//! `0x361` between the state `mkfs` left and the first commit. That is
//! the first mount recording a feature, not something a commit does, and
//! a writer that copied the behaviour would be setting a flag it does
//! not implement.
//!
//! # The checksum
//!
//! Over bytes `[32..4096]` — everything after the checksum itself, the
//! same rule tree blocks use. Reproduced on all seven captured
//! superblocks before anything here was written.

use crate::error::{Error, Result};
use crate::superblock::{ChecksumType, CSUM_SIZE};

/// Byte offsets within the superblock.
///
/// From `docs/transaction-format.md`, which measured them, and confirmed
/// against the captured commits.
pub mod offsets {
    /// The checksum, over everything after it.
    pub const CSUM: usize = 0x000;
    /// The transaction this superblock commits.
    pub const GENERATION: usize = 0x048;
    /// The root tree's address — what makes the commit real.
    pub const ROOT: usize = 0x050;
    /// The chunk tree's address. Moves only when a chunk is allocated.
    pub const CHUNK_ROOT: usize = 0x058;
    /// Bytes in use. Moves only when usage changes, and must equal the
    /// sum of every `BLOCK_GROUP_ITEM.used`.
    pub const BYTES_USED: usize = 0x078;
    /// The chunk tree's generation.
    pub const CHUNK_ROOT_GENERATION: usize = 0x0a4;
    /// Height of the root tree. Moves only when its depth changes.
    pub const ROOT_LEVEL: usize = 0x0c6;
    /// Zero with the free-space tree; tracks generation under
    /// `space_cache=v1`.
    pub const CACHE_GENERATION: usize = 0x22b;
    /// Tracks generation even when the UUID tree is untouched.
    pub const UUID_TREE_GENERATION: usize = 0x233;
    /// The offset this copy belongs at.
    ///
    /// Each copy records its own address, which is what lets a reader
    /// tell a genuine superblock from a stale image of one that was
    /// copied elsewhere. Verified on a real filesystem: the copy at
    /// 0x10000 holds 0x10000, the one at 0x4000000 holds 0x4000000.
    pub const BYTENR: usize = 0x30;
    /// Read-only compatibility flags. Bit 0 says a free-space tree
    /// exists; bit 1 says its contents can be trusted.
    pub const COMPAT_RO_FLAGS: usize = 0xb4;
    /// The first of four `btrfs_root_backup` slots.
    pub const ROOT_BACKUPS: usize = 0xb2b;
}

/// One `btrfs_root_backup` slot.
pub const ROOT_BACKUP_SIZE: usize = 168;

/// How many backup slots the ring holds.
pub const ROOT_BACKUP_SLOTS: u64 = 4;

/// One past the last byte of the backup ring.
pub const ROOT_BACKUPS_END: usize =
    offsets::ROOT_BACKUPS + (ROOT_BACKUP_SLOTS as usize) * ROOT_BACKUP_SIZE;

/// Byte offsets inside one `btrfs_root_backup` slot.
pub mod backup_offsets {
    pub const TREE_ROOT: usize = 0;
    pub const TREE_ROOT_GEN: usize = 8;
    pub const CHUNK_ROOT: usize = 16;
    pub const CHUNK_ROOT_GEN: usize = 24;
    pub const EXTENT_ROOT: usize = 32;
    pub const EXTENT_ROOT_GEN: usize = 40;
    pub const FS_ROOT: usize = 48;
    pub const FS_ROOT_GEN: usize = 56;
    pub const DEV_ROOT: usize = 64;
    pub const DEV_ROOT_GEN: usize = 72;
    pub const CSUM_ROOT: usize = 80;
    pub const CSUM_ROOT_GEN: usize = 88;
    pub const TOTAL_BYTES: usize = 96;
    pub const BYTES_USED: usize = 104;
    pub const NUM_DEVICES: usize = 112;
    /// After four unused 64-bit words.
    pub const TREE_ROOT_LEVEL: usize = 152;
    pub const CHUNK_ROOT_LEVEL: usize = 153;
    pub const EXTENT_ROOT_LEVEL: usize = 154;
    pub const FS_ROOT_LEVEL: usize = 155;
    pub const DEV_ROOT_LEVEL: usize = 156;
    pub const CSUM_ROOT_LEVEL: usize = 157;
}

/// One tree's root as a backup slot records it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackupRoot {
    pub bytenr: u64,
    pub generation: u64,
    pub level: u8,
}

/// What one backup-ring slot records: the roots a commit left, and the
/// superblock's size and usage at the time (`backup_super_roots`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackupRoots {
    pub tree: BackupRoot,
    pub chunk: BackupRoot,
    pub extent: BackupRoot,
    pub fs: BackupRoot,
    pub dev: BackupRoot,
    pub csum: BackupRoot,
    pub total_bytes: u64,
    pub bytes_used: u64,
    pub num_devices: u64,
}

/// Fill `generation`'s backup slot with `roots`, zeroing whatever else the
/// slot held, as the kernel's `backup_super_roots` does (#78). Call before
/// [`stamp_checksum`], which covers the ring.
pub fn write_backup(raw: &mut [u8], generation: u64, roots: &BackupRoots) {
    use backup_offsets as b;
    let at = backup_slot_offset(generation);
    let slot = &mut raw[at..at + ROOT_BACKUP_SIZE];
    slot.fill(0);
    let mut put = |off: usize, v: u64| slot[off..off + 8].copy_from_slice(&v.to_le_bytes());
    for (root, bytenr, gen) in [
        (&roots.tree, b::TREE_ROOT, b::TREE_ROOT_GEN),
        (&roots.chunk, b::CHUNK_ROOT, b::CHUNK_ROOT_GEN),
        (&roots.extent, b::EXTENT_ROOT, b::EXTENT_ROOT_GEN),
        (&roots.fs, b::FS_ROOT, b::FS_ROOT_GEN),
        (&roots.dev, b::DEV_ROOT, b::DEV_ROOT_GEN),
        (&roots.csum, b::CSUM_ROOT, b::CSUM_ROOT_GEN),
    ] {
        put(bytenr, root.bytenr);
        put(gen, root.generation);
    }
    put(b::TOTAL_BYTES, roots.total_bytes);
    put(b::BYTES_USED, roots.bytes_used);
    put(b::NUM_DEVICES, roots.num_devices);
    for (root, level) in [
        (&roots.tree, b::TREE_ROOT_LEVEL),
        (&roots.chunk, b::CHUNK_ROOT_LEVEL),
        (&roots.extent, b::EXTENT_ROOT_LEVEL),
        (&roots.fs, b::FS_ROOT_LEVEL),
        (&roots.dev, b::DEV_ROOT_LEVEL),
        (&roots.csum, b::CSUM_ROOT_LEVEL),
    ] {
        slot[level] = root.level;
    }
}

/// The whole superblock.
pub const SUPERBLOCK_SIZE: usize = 4096;

/// Which backup slot a generation writes.
///
/// `(generation - 1) mod 4`. The transaction document confirmed it
/// across six independent generations, and the captured commits confirm
/// it again from the other direction: a filesystem going from generation
/// 6 to 8 — a mount and an unmount, each committing — changed slots 2
/// and 3, which is `(7-1) mod 4` and `(8-1) mod 4`.
///
/// Generation 0 is not a commit and has no slot, so it is not special
/// cased: nothing calls this with one.
pub fn backup_slot(generation: u64) -> usize {
    ((generation.wrapping_sub(1)) % ROOT_BACKUP_SLOTS) as usize
}

/// Where a generation's backup slot begins.
pub fn backup_slot_offset(generation: u64) -> usize {
    offsets::ROOT_BACKUPS + backup_slot(generation) * ROOT_BACKUP_SIZE
}

/// What a commit sets in the superblock.
///
/// Only `generation` and `root` are required of every commit. The rest
/// are `None` when the commit does not move them, which is the common
/// case — a commit that changes no usage, no tree depth and allocates no
/// chunk leaves all four alone, and writing them back unchanged would be
/// the same bytes but a worse description of what happened.
#[derive(Debug, Clone, Default)]
pub struct Commit {
    /// The new transaction number. Every commit.
    pub generation: u64,
    /// The new root tree address. Every commit.
    pub root: u64,
    /// Set when the root tree's depth changed.
    pub root_level: Option<u8>,
    /// Set when usage changed. Must equal the sum of every
    /// `BLOCK_GROUP_ITEM.used`.
    pub bytes_used: Option<u64>,
    /// Set when a chunk was allocated.
    pub chunk_root: Option<u64>,
    /// Set alongside `chunk_root`.
    pub chunk_root_generation: Option<u64>,
    /// Set to `true` when the transaction did NOT maintain the
    /// free-space tree.
    ///
    /// The free-space tree is a cache of the complement of the extent
    /// tree, and it has a validity bit precisely so it can be marked
    /// untrustworthy: the kernel then rebuilds it on the next
    /// read-write mount rather than believing it. A writer that moves
    /// blocks without updating the cache MUST say so, or the filesystem
    /// carries a cache describing where things used to be — which
    /// `btrfs check` reports as "cache appears valid but isn't".
    ///
    /// Clearing the bit is not a workaround; it is the state the format
    /// defines for exactly this.
    pub invalidate_free_space_tree: bool,
}

/// Apply a commit to a superblock, in place, and re-checksum it.
///
/// The result is the superblock as it should be written to every copy —
/// except for each copy's own `bytenr`, which is that copy's address and
/// is not this function's business.
///
/// # What it does not do
///
/// **It does not fill the backup slot.** The ring records the roots of
/// each of the last four commits, and those are in the root tree the
/// commit writes, which this function is not given.
/// [`crate::fs::Filesystem::commit`] reads them out of the new root tree
/// and fills the slot with [`write_backup`] (#78).
///
/// # Errors
///
/// [`Error::UnsupportedFeature`] if the buffer is not a whole
/// superblock.
pub fn apply(raw: &mut [u8], csum_type: ChecksumType, commit: &Commit) -> Result<()> {
    if raw.len() < SUPERBLOCK_SIZE {
        return Err(Error::UnsupportedFeature(format!(
            "a superblock is {SUPERBLOCK_SIZE} bytes and this is {}",
            raw.len()
        )));
    }

    put64(raw, offsets::GENERATION, commit.generation);
    put64(raw, offsets::ROOT, commit.root);

    // Tracks generation even when the UUID tree itself was not touched,
    // which is why it is not optional and not derived from anything the
    // caller passes.
    put64(raw, offsets::UUID_TREE_GENERATION, commit.generation);

    if let Some(level) = commit.root_level {
        raw[offsets::ROOT_LEVEL] = level;
    }
    if let Some(used) = commit.bytes_used {
        put64(raw, offsets::BYTES_USED, used);
    }
    if let Some(chunk_root) = commit.chunk_root {
        put64(raw, offsets::CHUNK_ROOT, chunk_root);
    }
    if let Some(gen) = commit.chunk_root_generation {
        put64(raw, offsets::CHUNK_ROOT_GENERATION, gen);
    }

    if commit.invalidate_free_space_tree {
        use crate::superblock::compat_ro::FREE_SPACE_TREE_VALID;
        let flags = u64::from_le_bytes(
            raw[offsets::COMPAT_RO_FLAGS..offsets::COMPAT_RO_FLAGS + 8]
                .try_into()
                .expect("8 bytes"),
        );
        put64(
            raw,
            offsets::COMPAT_RO_FLAGS,
            flags & !FREE_SPACE_TREE_VALID,
        );
    }

    stamp_checksum(raw, csum_type);
    Ok(())
}

/// Stamp the address this copy belongs at.
///
/// The copies are not identical images: each names its own offset, and
/// [`crate::superblock::Superblock::parse_at`] rejects one found where it
/// does not claim to be. So a commit cannot write one buffer to all three — it
/// re-stamps and re-checksums per copy.
///
/// Call before [`stamp_checksum`], which covers this field.
pub fn set_bytenr(raw: &mut [u8], offset: u64) {
    put64(raw, offsets::BYTENR, offset);
}

/// Compute and store the superblock's checksum.
///
/// Over everything after the checksum itself, so it can only be done
/// once every other field is final. Same rule as a tree block.
pub fn stamp_checksum(raw: &mut [u8], csum_type: ChecksumType) {
    let digest = csum_type.digest(&raw[CSUM_SIZE..SUPERBLOCK_SIZE]);
    raw[..CSUM_SIZE].copy_from_slice(&digest);
}

/// The longest label: the field is 256 bytes and keeps a terminator.
pub const MAX_LABEL_BYTES: usize = 255;

/// Set the label on `dev`: every superblock copy of this filesystem gets
/// it, each with a fresh checksum, the mirrors first and the primary last.
///
/// The label is the superblock's and nothing else's, so this is the whole
/// change: no tree holds it and no transaction is needed. Nothing else in
/// a copy changes, its generation included.
///
/// # Errors
///
/// [`Error::UnsupportedFeature`] for a label longer than
/// [`MAX_LABEL_BYTES`], or one holding a NUL, and for a filesystem that
/// spans more than one device, whose other devices carry the label too;
/// whatever reading or writing the device returns.
pub fn set_label(dev: &dyn fs_core::BlockDevice, label: &str) -> Result<()> {
    use crate::superblock::{offsets as o, Superblock};
    if label.len() > MAX_LABEL_BYTES || label.contains('\0') {
        return Err(Error::UnsupportedFeature(format!(
            "a label is at most {MAX_LABEL_BYTES} bytes, with no NUL; this one is {}",
            label.len()
        )));
    }
    const COPIES: [u64; 3] = [64 << 10, 64 << 20, 256 << 30];
    let mut primary = vec![0u8; SUPERBLOCK_SIZE];
    dev.read_at(COPIES[0], &mut primary)?;
    let sb = Superblock::parse(&primary)?;
    if sb.num_devices > 1 {
        return Err(Error::UnsupportedFeature(format!(
            "this filesystem spans {} devices, and every one of them carries the label",
            sb.num_devices
        )));
    }
    let size = dev.size_bytes();
    let mut copies = Vec::new();
    for &at in &COPIES {
        if at + SUPERBLOCK_SIZE as u64 > size {
            continue;
        }
        let mut raw = vec![0u8; SUPERBLOCK_SIZE];
        dev.read_at(at, &mut raw)?;
        // Only a copy of THIS filesystem: a stale superblock from an
        // earlier one at a mirror's offset is not ours to rewrite.
        match Superblock::parse(&raw) {
            Ok(copy) if copy.fsid == sb.fsid => copies.push((at, raw)),
            _ => {}
        }
    }
    for (at, raw) in copies.iter_mut().rev() {
        raw[o::LABEL..o::LABEL + 256].fill(0);
        raw[o::LABEL..o::LABEL + label.len()].copy_from_slice(label.as_bytes());
        stamp_checksum(raw, sb.csum_type);
        dev.write_at(*at, raw)?;
        dev.flush()?;
    }
    Ok(())
}

fn put64(raw: &mut [u8], at: usize, value: u64) {
    raw[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slot rule, including the wrap that a single commit would not
    /// exercise.
    #[test]
    fn the_backup_slot_is_the_generation_before_it_modulo_four() {
        assert_eq!(backup_slot(1), 0);
        assert_eq!(backup_slot(2), 1);
        assert_eq!(backup_slot(3), 2);
        assert_eq!(backup_slot(4), 3);
        assert_eq!(backup_slot(5), 0, "and round again");

        // The pair the captured commits showed: generation 6 to 8 wrote
        // slots 2 and 3.
        assert_eq!(backup_slot(7), 2);
        assert_eq!(backup_slot(8), 3);
    }

    /// Slot offsets are 168 apart and the ring stays inside the
    /// superblock.
    #[test]
    fn the_ring_fits_in_the_superblock() {
        assert_eq!(backup_slot_offset(1), offsets::ROOT_BACKUPS);
        assert_eq!(
            backup_slot_offset(2),
            offsets::ROOT_BACKUPS + ROOT_BACKUP_SIZE
        );
        let last = backup_slot_offset(4) + ROOT_BACKUP_SIZE;
        assert!(
            last <= SUPERBLOCK_SIZE,
            "the four slots end at {last}, past the superblock"
        );
    }

    /// A buffer that is not a superblock is refused rather than indexed
    /// into.
    #[test]
    fn a_short_buffer_is_refused() {
        let mut small = vec![0u8; 512];
        assert!(apply(&mut small, ChecksumType::Crc32c, &Commit::default()).is_err());
    }
}
