//! Mounted filesystem handle.
//!
//! Ties the superblock, chunk map, B-tree reader and item parsers into
//! the operations a consumer wants: open a device, resolve a path, list
//! a directory, read a file.
//!
//! # Getting to the data
//!
//! Btrfs does not name the fs tree in its superblock, so mounting is a
//! four-step bootstrap and every step must succeed before the next is
//! even addressable:
//!
//! 1. Parse the superblock at 64 KiB.
//! 2. Build a chunk map from the `sys_chunk_array` embedded in it —
//!    enough to reach the chunk tree and nothing else.
//! 3. Walk the chunk tree through that partial map, folding every chunk
//!    item in to complete it.
//! 4. Read the root tree, find the `ROOT_ITEM` for the fs tree, and take
//!    the tree's root address from it.
//!
//! # What is deliberately refused
//!
//! Returning plausible-but-wrong file contents is the one failure a
//! caller cannot detect, so these are errors rather than best efforts:
//!
//! Compressed extents are NOT in that list. All three btrfs codecs —
//! zlib, LZO and zstd — are decoded, in [`crate::compression`]. This
//! module used to say decompression was unimplemented, which was true
//! when it was written and has not been for some time.
//!
//! - **Encoded extents** — encrypted or otherwise transformed.
//! - **A dirty log.** `log_root` being set means the tree on disk is not
//!   the whole story.
//!
//! Holes read as zeros, which is what they are.

use crate::btree::{Tree, TreeGeometry};
use crate::chunk::{ChunkMap, DiskKey};
use crate::compression::{self, Compression};
use crate::dir::{self, DirEntry, DIR_INDEX_KEY, XATTR_ITEM_KEY};
use crate::error::{Error, Result};
use crate::inode::{Inode, FIRST_FREE_OBJECTID, INODE_ITEM_KEY};
use crate::superblock::{le64, Superblock};
use crate::xattr::{self, XattrEntry};
use fs_core::{BlockDevice, BlockRead};
use std::collections::BTreeMap;
use std::sync::Arc;

/// `BTRFS_FS_TREE_OBJECTID` — the subvolume holding the default
/// filesystem namespace. The one copy is `chunk::objectid`.
pub use crate::chunk::objectid::FS_TREE as FS_TREE_OBJECTID;

/// One item of the root tree: `(objectid, key_type, offset, data)`.
///
/// Named because the tuple is what the root tree actually holds — a key
/// in three parts and an opaque body whose meaning depends on the type —
/// and a struct here would invent a shape the format does not have.
pub type RootTreeItem = (u64, u8, u64, Vec<u8>);

/// `BTRFS_ROOT_ITEM_KEY`.
pub const ROOT_ITEM_KEY: u8 = 132;

/// `BTRFS_EXTENT_DATA_KEY` — a file's data, inline or by reference.
pub const EXTENT_DATA_KEY: u8 = 108;

/// Byte offsets within `struct btrfs_root_item`.
///
/// The item opens with an embedded `btrfs_inode_item` of 160 bytes,
/// followed by `generation` and `root_dirid` before the root address.
///
/// THE one copy. There were three — here, in `subvol`, and in
/// `transaction` — and only one of them carried the note below about
/// offset 160, which is the field a writer gets wrong.
pub mod root_item {
    /// `u64`. The transaction this tree was last written in.
    ///
    /// AT 160, after the embedded `btrfs_inode_item`. Offset 16 is
    /// inside that inode and holds something else entirely — a
    /// `ROOT_ITEM` whose generation was written there leaves the real
    /// field stale, and the kernel refuses the tree it names with
    /// "parent transid verify failed". Which is exactly what `btrfs
    /// check` said before this was measured.
    pub const GENERATION: usize = 160;
    /// `u64`. The inode number of the tree's top directory, 256 in every
    /// tree the kernel makes.
    pub const ROOT_DIRID: usize = 168;
    /// `u64`. The tree's root block.
    ///
    /// Measured against a real filesystem, not counted from the struct:
    /// the `ROOT_ITEM` for the extent tree holds the address the
    /// superblock's own walk reaches. A wrong value here yields an
    /// address whose tree block fails its own identity check rather
    /// than producing plausible garbage.
    pub const BYTENR: usize = 176;
    /// `u64`. The generation this subvolume was last snapshotted at, or
    /// zero if it never was.
    pub const LAST_SNAPSHOT: usize = 200;
    /// `u64`. Bit 0 is `BTRFS_ROOT_SUBVOL_RDONLY`.
    pub const FLAGS: usize = 208;
    /// The height of the tree this item names.
    pub const LEVEL: usize = 238;
    /// `u64`. A second copy of [`GENERATION`], right after `level`.
    ///
    /// The kernel compares the two when it reads a root, and when they
    /// differ it takes the item for one written by a kernel that predates
    /// the extended fields and RESETS them — the subvolume's UUIDs and
    /// times — logging "mismatching generation and generation_v2". Every
    /// writer of [`GENERATION`] writes this too, as `btrfs_update_root`
    /// does. Found by `tests/crash_consistency.rs` (#195), which is the
    /// first test here to read the kernel's log after mounting a commit of
    /// this crate.
    pub const GENERATION_V2: usize = 239;
    /// The smallest item any of these fields can be read out of.
    pub const MIN_SIZE: usize = FLAGS + 8;
}

/// The root block address the root tree records for `objectid`.
///
/// This existed three times — `block_group::tree_root`,
/// `write::extent_tree_root`, and inline in `fs::open_pool` for the FS
/// tree — and **the three did not agree**, in two ways that no test
/// distinguishes because a real `ROOT_ITEM` is 439 bytes and none of
/// the disagreements can trigger on one:
///
/// * two required `data.len() > root_item::LEVEL` (238) and one
///   required `data.len() > root_item::BYTENR + 8` (184), so a
///   truncated item of 185..=238 bytes was accepted by one and rejected
///   by the others;
/// * two stopped at the first matching `ROOT_ITEM` and one kept
///   scanning and took the **last**, so a root tree holding two items
///   for the same objectid would have been read differently depending
///   on which caller asked.
///
/// This settles both. The bound is `BYTENR + 8` — the bytes actually
/// read — because a bound of `LEVEL` refuses items this function could
/// answer from. And it takes the **first** match and stops, which is
/// what a tree with one `ROOT_ITEM` per objectid means; if there are
/// two, the tree is already malformed and reading further does not make
/// the answer better.
///
/// `tree` walks the root tree, so callers differ only in which read
/// closure they built it from — which is the only thing that ever
/// genuinely differed between the three copies.
///
/// # `Ok(None)` means absent, not "an error occurred and was discarded"
///
/// A tree with no `ROOT_ITEM` for `objectid` and a walk that failed
/// partway through it are different facts, and this signature is what
/// keeps a caller from being able to conflate them. `Ok(None)` is only
/// reached by a walk that completed and found nothing; a read failure,
/// a checksum mismatch or a corrupt block encountered while walking
/// comes back through `for_each`'s `?` as `Err`, never as `None`. A
/// caller for whom absence is not legitimate should not call `.ok()` on
/// this — see [`required_root_item_target`], which turns `Ok(None)`
/// into the same named error this function used to return directly, and
/// exists so that turning "absent" into an error and turning "any
/// failure" into an error are not the same line of code.
///
/// # Errors
///
/// Propagates a genuine failure walking the root tree. Never returns an
/// error to report that `objectid` is simply not present — see above.
/// The sector-aligned window that covers `[logical, logical+len)`,
/// as `(first, span)`.
///
/// Pulled out of [`Filesystem::read_data_verified`] so the arithmetic
/// can be tested directly rather than only through a fully mounted
/// filesystem, which is what let this go unwitnessed the first time.
///
/// `None` on any overflow, which the caller turns into
/// [`Error::UnmappedLogical`]. `logical` is an extent address read off
/// disk, so a corrupted or hostile image can put any of these within
/// reach:
///
/// - `logical + len`, guarded by `checked_add`;
/// - the round-up to a sector boundary, `div_ceil(sector) * sector` --
///   `div_ceil` cannot overflow, division only shrinks, but the
///   multiplication that rounds back up can, for an `end` within one
///   sector of `u64::MAX`. This is the guard that matters: `checked_mul`
///   catches every case, and dropping it is not covered by the
///   subtraction below despite first appearances.
/// - `last - first`, guarded by `checked_sub` -- and, given the
///   multiplication above already succeeded (so `last` is the true,
///   un-wrapped ceiling), THIS CANNOT ACTUALLY UNDERFLOW: ceiling and
///   floor give `first <= logical <= end <= last` unconditionally, so
///   `last >= first` always holds once `checked_mul` returned `Some`.
///   It stays as a second line of defence in case that invariant is
///   ever disturbed by a future edit to the lines above it, not because
///   it is load-bearing today.
///
/// Provable, and worth being precise about because the obvious intuition
/// is the opposite one and it is wrong: `checked_sub` does NOT generally
/// stand in for `checked_mul`. Picking `logical` near `u64::MAX` makes
/// `first` huge, and a wrapped `last` (small, having overflowed past
/// zero) then IS caught as an apparent underflow -- which looks like the
/// subtraction is doing the real work. But `logical = 0` with an
/// enormous `len` reaches the identical multiplication overflow while
/// `first` stays `0`, and a wrapped `last` of `0` gives
/// `checked_sub(0, 0) = Some(0)`: a small, entirely plausible span,
/// silently wrong rather than refused. Both shapes are regression-tested
/// below.
///
/// A wrapped `span` that got past both guards would reach `vec![0u8;
/// span as usize]` as something near `u64::MAX` bytes -- an allocation
/// that either aborts the process or, on a 32-bit `usize`, truncates and
/// wraps again into an undersized buffer a later write overruns. See
/// rust-fs-btrfs#114.
fn sector_aligned_span(logical: u64, len: u64, sector: u64) -> Option<(u64, u64)> {
    let first = logical - logical % sector;
    let end = logical.checked_add(len)?;
    let last = end.div_ceil(sector).checked_mul(sector)?;
    let span = last.checked_sub(first)?;
    Some((first, span))
}

/// The two named trees a mount reads from the root tree: the fs tree,
/// which must exist, and the checksum tree, which legitimately may not.
///
/// One call rather than two `root_item_target` calls at the mount site,
/// so this decision is made once and named rather than reproduced
/// inline where it is easy to weaken by editing one arm and not the
/// other.
///
/// # `Ok((_, None))` means absent, not "an error occurred and was
/// discarded"
///
/// The checksum tree's `Ok(None)` is reached only when the walk for it
/// completed and genuinely found no `ROOT_ITEM` -- every filesystem the
/// kernel makes lacks one exactly this way, pre-checksumming or with the
/// feature never enabled. Any OTHER failure looking for it -- an I/O
/// error, a corrupt block, anything `for_each` can fail on while walking
/// further into the root tree than the fs-tree lookup needed to go --
/// comes back as `Err` and is propagated by `?`, never folded into this
/// `None`.
///
/// It used to be `root_item_target(...).ok()` at the call site, which
/// discarded an `Err` exactly as it discarded a legitimate absence: a
/// corrupt or unreadable root tree was indistinguishable from "no
/// checksum tree," and every subsequent read for the rest of the mount
/// went unverified with nothing logged. See rust-fs-btrfs#114, and
/// `fs_and_csum_tree_roots_tests` for the regression -- a fixture where
/// the fs-tree lookup succeeds without ever reaching the block that
/// fails, so the failure can only be observed if the csum-tree lookup's
/// own error is not swallowed.
///
/// # Errors
///
/// [`Error::BadSuperblock`] if the fs tree itself is absent. Any error
/// encountered walking for either tree.
fn fs_and_csum_tree_roots(tree: &crate::btree::Tree, root_tree: u64) -> Result<(u64, Option<u64>)> {
    Ok((
        required_root_item_target(tree, root_tree, FS_TREE_OBJECTID)?,
        root_item_target(tree, root_tree, crate::csum::CSUM_TREE_OBJECTID)?,
    ))
}

pub(crate) fn root_item_target(
    tree: &crate::btree::Tree,
    root_tree: u64,
    objectid: u64,
) -> Result<Option<u64>> {
    let mut root = None;
    tree.for_each(root_tree, &mut |key: &DiskKey, data: &[u8]| {
        if key.objectid == objectid
            && key.key_type == ROOT_ITEM_KEY
            && data.len() >= root_item::BYTENR + 8
        {
            root = Some(le64(data, root_item::BYTENR));
            return Ok(false);
        }
        Ok(true)
    })?;
    Ok(root)
}

/// [`root_item_target`], refusing an absent tree.
///
/// For a tree every caller of this function needs to exist — the fs
/// tree, or any tree reached through [`crate::block_group`]'s public
/// lookup, which documents absence as an error — `Ok(None)` is itself
/// the failure, and a bare `?` on `root_item_target` would let it
/// through disguised as success. This is the one place that distinction
/// is made, so every "tree must exist" caller reports the same message.
pub(crate) fn required_root_item_target(
    tree: &crate::btree::Tree,
    root_tree: u64,
    objectid: u64,
) -> Result<u64> {
    root_item_target(tree, root_tree, objectid)?.ok_or_else(|| {
        Error::BadSuperblock(format!(
            "the root tree holds no ROOT_ITEM for tree {objectid}"
        ))
    })
}

/// Byte offsets within `struct btrfs_file_extent_item`.
///
/// The full field list is kept even where a read-only driver does not
/// consult every one, because the offsets that follow are only checkable
/// against the format documentation when the fields between them are
/// named too.
#[allow(dead_code)]
pub(crate) mod file_extent {
    /// Generation of the transaction that created it.
    pub const GENERATION: usize = 0;
    /// Decoded size of the extent's data.
    pub const RAM_BYTES: usize = 8;
    /// Compression algorithm, 0 for none.
    pub const COMPRESSION: usize = 16;
    /// Encryption, 0 for none.
    pub const ENCRYPTION: usize = 17;
    /// Other encoding, 0 for none.
    pub const OTHER_ENCODING: usize = 18;
    /// 0 = inline, 1 = regular, 2 = prealloc.
    pub const TYPE: usize = 20;
    /// Inline data begins here.
    pub const INLINE_DATA: usize = 21;
    /// Physical address of the extent, or 0 for a hole.
    pub const DISK_BYTENR: usize = 21;
    /// Bytes occupied on disk.
    pub const DISK_NUM_BYTES: usize = 29;
    /// Offset into the extent at which this reference starts.
    pub const OFFSET: usize = 37;
    /// Logical length of this reference.
    pub const NUM_BYTES: usize = 45;
    /// Size of the non-inline header.
    pub const REGULAR_SIZE: usize = 53;
}

/// Extent storage kinds.
const EXTENT_INLINE: u8 = 0;
pub(crate) const EXTENT_REGULAR: u8 = 1;
const EXTENT_PREALLOC: u8 = 2;

/// One resolved piece of a file's contents.
enum Piece<'a> {
    /// Data stored inside the item itself, already decoded.
    Inline(std::borrow::Cow<'a, [u8]>),
    /// Data on disk: logical address, and the length to read.
    Regular { logical: u64, len: u64 },
    /// A compressed run on disk that must be decoded whole, then sliced.
    ///
    /// The compressed unit is the entire extent, so unlike [`Piece::Regular`]
    /// the reference's `offset` cannot be folded into the address — it
    /// indexes the *decoded* bytes, and seeking by it on disk would land
    /// in the middle of a compressed stream. See [`crate::compression`].
    Compressed {
        /// Start of the compressed run.
        logical: u64,
        /// Its length on disk.
        disk_len: u64,
        /// What the whole run decodes to.
        ram_len: u64,
        /// Where this reference starts within the decoded bytes.
        offset: u64,
        /// How much of the decoded bytes this reference covers.
        len: u64,
        algo: Compression,
    },
    /// A hole: nothing on disk behind this range.
    ///
    /// Carries no length: the output buffer is zeroed before any extent
    /// is copied into it, so a region with nothing to copy is already
    /// correct. Naming the case explicitly rather than falling through
    /// keeps the reason visible at the match site.
    Hole,
    /// An unwritten preallocated extent: blocks reserved at `logical` for
    /// `len` bytes of the file, reading as zeros.
    ///
    /// Not a hole. The space is this file's and the extent tree has an
    /// item for it, so the write planner has to see it -- it was dropped
    /// with the holes, and a write to a `fallocate`d range was refused as
    /// a hole and `can_write_in_place` answered yes (#74).
    Preallocated { logical: u64, len: u64 },
}

/// One extent of a file, located both in the file and on the volume.
pub(crate) struct FileExtent {
    /// Offset within the file where this extent begins.
    pub start: u64,
    /// How much of the file it covers.
    pub len: u64,
    /// Where its bytes are, when they can be written in place at all.
    pub logical: Option<u64>,
    /// Start of the whole extent run, which is what the extent tree
    /// keys its reference count by.
    pub extent_start: u64,
    /// Whether the bytes on disk are compressed.
    pub compressed: bool,
}

/// A mounted Btrfs filesystem.
pub struct Filesystem {
    pub(crate) device: Arc<dyn BlockRead>,
    /// The devices of a pool, by the id chunk stripes reference.
    ///
    /// Empty for a single-device filesystem, where [`Self::device`] is
    /// everything and a mapping's `devid` cannot be anything else. When
    /// a filesystem spans several devices this holds all of them,
    /// including the one in [`Self::device`], because a stripe names
    /// the disk it is on and reading it from any other returns whatever
    /// happens to be at that offset.
    pub(crate) devices: BTreeMap<u64, Arc<dyn BlockRead>>,
    /// The same device again, present only when the volume was opened
    /// for writing. Kept separately so that "can this mount write" is a
    /// property of the type: the write path cannot compile without
    /// going through this field.
    pub(crate) writable: Option<Arc<dyn BlockDevice>>,
    pub(crate) sb: Superblock,
    pub(crate) map: ChunkMap,
    pub(crate) fs_tree_root: u64,
    /// The csum tree's root, when the filesystem has one.
    ///
    /// `None` only for a filesystem whose root tree holds no ROOT_ITEM
    /// for it, which the kernel does not produce; the read path then
    /// verifies nothing, which is what it did everywhere before.
    csum_tree_root: Option<u64>,
    /// Tree blocks this mount has read, by logical address.
    ///
    /// The fs tree was loaded whole at mount, every item into a map, so
    /// that a `stat` would not re-read the interior nodes above it. That
    /// made opening a volume cost what the volume held: 2573 device reads
    /// and 417 ms for 20,000 files before a single question (#67). Items
    /// are now found by descending the tree when asked, and the nodes a
    /// descent reads are kept here, so the interior nodes every path
    /// passes through are read, and verified, once. Bounded, least recently
    /// used out.
    nodes: std::sync::Mutex<NodeCache>,
}

/// Tree blocks a mount keeps: 1024, 16 MiB at the default 16 KiB node.
pub const NODE_CACHE_BLOCKS: usize = 1024;

/// A least-recently-used set of verified tree blocks by logical address.
pub(crate) struct NodeCache {
    capacity: usize,
    blocks: std::collections::HashMap<u64, (crate::btree::TreeBlock, u64)>,
    tick: u64,
}

impl NodeCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            blocks: std::collections::HashMap::new(),
            tick: 0,
        }
    }

    fn get(&mut self, logical: u64) -> Option<crate::btree::TreeBlock> {
        self.tick += 1;
        let tick = self.tick;
        self.blocks.get_mut(&logical).map(|(block, used)| {
            *used = tick;
            block.clone()
        })
    }

    fn put(&mut self, logical: u64, block: &crate::btree::TreeBlock) {
        if self.capacity == 0 {
            return;
        }
        if self.blocks.len() >= self.capacity && !self.blocks.contains_key(&logical) {
            if let Some(&victim) = self
                .blocks
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(k, _)| k)
            {
                self.blocks.remove(&victim);
            }
        }
        self.tick += 1;
        self.blocks.insert(logical, (block.clone(), self.tick));
    }

    fn clear(&mut self) {
        self.blocks.clear();
    }
}

/// The longest a symbolic link's target can be.
///
/// `PATH_MAX`. A link's target is a path and the operating system will
/// not take a longer one, while the inode's declared size is a raw
/// `le64`.
pub const MAX_SYMLINK_TARGET: u64 = 4096;

/// The largest file [`Filesystem::read_file`] materialises: 1 GiB.
///
/// A ceiling, because a fallible reservation is not a bound where memory
/// is overcommitted. `try_reserve_exact` can succeed for a range no
/// process could fill, and zeroing it then commits every page, so a
/// corrupt `inode.size` below the volume's size still ended in the OOM
/// killer rather than an error (Greptile on #150). Anything larger is
/// read in pieces with [`Filesystem::read_at`], which the C ABI already
/// does for every read.
pub const MAX_WHOLE_FILE_READ: u64 = 1 << 30;

/// An extent that does not hold the bytes it claims to cover.
fn short_extent(ino: u64, kind: &str) -> Error {
    Error::BadSuperblock(format!(
        "inode {ino}: a {kind} extent is shorter than the range it covers"
    ))
}

/// How many blocks a mount caches by default: **none**.
///
/// Not an oversight, and not a placeholder. Measured — see
/// `docs/read-path-cost.md` — a block cache buys this driver nothing
/// and costs it something:
///
/// - Tree blocks are held whole, by logical address, in the mount's node
///   cache (`NODE_CACHE_BLOCKS`), so the repeat metadata reads a sector
///   cache would serve are already served above it.
/// - Metadata is read a node at a time, and a node is `nodesize` —
///   typically four sectors. Caching by sector turns one call into
///   four, so on the `rich` fixture the cached mount asked the device
///   for 14 reads where the uncached one asked for 4.
///
/// [`Filesystem::mount_with_cache`] still exists so the measurement can
/// take both passes, and so the decision can be re-taken against a
/// number.
pub const DEFAULT_CACHE_BLOCKS: usize = 0;

impl Filesystem {
    /// Open `device` as a Btrfs filesystem.
    pub fn mount(device: Arc<dyn BlockRead>) -> Result<Self> {
        Self::mount_with_cache(device, DEFAULT_CACHE_BLOCKS)
    }

    /// Open `device` for reading, caching `blocks` metadata blocks.
    ///
    /// # Why the cache is built here and not by the caller
    ///
    /// It is sized in blocks, and the block size is the filesystem's.
    /// A caller wanting to wrap the device itself would have to parse a
    /// superblock first to know what to wrap it with — which is what
    /// this does, once, before wrapping.
    ///
    /// # What it is for
    ///
    /// Every lookup descends from the root of the filesystem tree, so
    /// the nodes near that root are read again for each path, and the
    /// chunk map is consulted for every logical address translated.
    /// None of those bytes change during a mount.
    ///
    /// `blocks` of zero disables it, which is what the measurement in
    /// `tests/read_path_cost.rs` uses to take its baseline.
    pub fn mount_with_cache(device: Arc<dyn BlockRead>, blocks: usize) -> Result<Self> {
        if blocks == 0 {
            return Self::open(device, None);
        }
        // The sector size is not known until a superblock has been
        // parsed, and the superblock is at a fixed offset, so this one
        // read goes to the device directly.
        let (sb, copy) = crate::superblock::read_superblock(&*device)?;
        let sectorsize = u64::from(sb.sectorsize);

        // CACHED BY SECTOR RATHER THAN BY NODE. A node is `nodesize`,
        // typically 16 KiB, and caching whole nodes would make the unit
        // four times larger than the smallest useful read. Sectors are
        // the finer unit and a node is then four cached blocks, stitched
        // by the cache itself.
        let device: Arc<dyn BlockRead> =
            fs_core::CachingDevice::read_only(device, sectorsize, blocks);
        // The selection just read is the one the mount uses. Reading it
        // again through the cache fetched the superblocks twice, and
        // `read_path_cost` -- which asserts a cache never fetches more bytes
        // than the uncached mount -- failed on it once it ran against a
        // fixture (#70).
        Self::open_pool(
            device,
            BTreeMap::new(),
            None,
            Some((sb, copy)),
            false,
        )
    }

    /// Mount the committed trees of a read-only filesystem even when the
    /// superblock names an unreplayed log tree.
    ///
    /// The main trees are copy-on-write committed state. A non-empty log tree
    /// contains additional fsync-visible changes that still need replay, so
    /// callers must surface that limitation to the user. This entry point is
    /// for forensic read-only access where returning the committed generation
    /// is preferable to refusing the entire volume.
    pub fn mount_committed_with_cache(
        device: Arc<dyn BlockRead>,
        blocks: usize,
    ) -> Result<Self> {
        if blocks == 0 {
            return Self::open_pool(device, BTreeMap::new(), None, None, true);
        }
        let (sb, copy) = crate::superblock::read_superblock(&*device)?;
        let sectorsize = u64::from(sb.sectorsize);
        let device: Arc<dyn BlockRead> =
            fs_core::CachingDevice::read_only(device, sectorsize, blocks);
        Self::open_pool(
            device,
            BTreeMap::new(),
            None,
            Some((sb, copy)),
            true,
        )
    }

    /// Open `device` for reading **and writing**.
    ///
    /// Writing is opt-in rather than inferred from the device being
    /// writable: a driver able to write should not do so merely because
    /// nothing stopped it.
    ///
    /// The refusal of a non-empty log tree applies here as to a
    /// read-only mount, and matters more — writing to a volume whose log
    /// holds changes the trees have not seen would layer new data on top
    /// of state that is about to be replayed over it.
    ///
    /// A volume whose chosen superblock is not the primary copy -- the
    /// primary is damaged, or older than a mirror -- is refused: something
    /// happened to it, and committing on top of that is not a decision to
    /// make without the user. It still mounts read-only (#90). The check
    /// is made on the superblock the mount then uses, in `open_pool`.
    ///
    /// A volume with a `compat_ro` feature this driver does not maintain
    /// is refused here, and can still be mounted read-only; see
    /// [`crate::superblock::refuse_unmaintained_compat_ro`] (#72).
    pub fn mount_rw(device: Arc<dyn BlockDevice>) -> Result<Self> {
        if !device.is_writable() {
            return Err(Error::ReadOnly);
        }
        let fs = Self::open(device.clone(), Some(device))?;
        crate::superblock::refuse_unmaintained_compat_ro(fs.sb.compat_ro_flags)?;
        // A SEED DEVICE IS READ-ONLY BY CONSTRUCTION (#76). Another
        // filesystem is layered on it, and writes belong to that sprout;
        // writing the seed changes blocks the sprout depends on being
        // immutable. It still mounts read-only.
        if fs.sb.is_seeding() {
            return Err(Error::UnsupportedFeature(
                "this device is a seed for another filesystem, which can be read but not \
                 written; write to the filesystem sprouted from it"
                    .into(),
            ));
        }
        Ok(fs)
    }

    /// Whether this mount can write.
    pub fn is_writable(&self) -> bool {
        self.writable.is_some()
    }

    /// Where a mapped range lands on the device, once it is known to be
    /// on it.
    ///
    /// A chunk's `btrfs_stripe.offset` is a raw `u64` off the disk, and
    /// `Chunk::validate_geometry` checks it for sector alignment and
    /// nothing else -- not against `dev_item.total_bytes`, not against
    /// the device. So every tree block and every data byte this crate
    /// writes could land at an offset the image chose. Against a
    /// file-backed image that is not an error at all: `write_at` on a
    /// file extends it, so a chunk at `stripe.offset = 2^60` grows a
    /// mounted image toward an exabyte. On a `CallbackDevice` the
    /// offset goes straight to the host.
    ///
    /// `commit::superblock_copy_fits` applies exactly this rule to the
    /// superblock copies; nothing else did.
    fn writable_span(device: &Arc<dyn BlockDevice>, physical: u64, len: usize) -> Result<()> {
        let end = physical.checked_add(len as u64).ok_or_else(|| {
            Error::BadSuperblock(format!(
                "a chunk maps a write to {physical}, which ends past the address space"
            ))
        })?;
        let device_bytes = device.size_bytes();
        if end > device_bytes {
            return Err(Error::BadSuperblock(format!(
                "a chunk maps a write to [{physical}, {end}) on a device of {device_bytes} bytes"
            )));
        }
        Ok(())
    }

    /// Every `(physical, length)` a write of `len` bytes at `logical`
    /// lands on: each mirror, split at chunk-stripe boundaries, each span
    /// checked against the device before anything is written.
    pub(crate) fn mirror_spans(
        device: &Arc<dyn BlockDevice>,
        map: &ChunkMap,
        logical: u64,
        len: usize,
    ) -> Result<Vec<(u64, usize, usize)>> {
        let mut spans = Vec::new();
        for mirror in 0..map.mirrors_at(logical)? {
            let mut done = 0usize;
            while done < len {
                let m = map.map_mirror(logical + done as u64, mirror)?;
                let n = (m.len as usize).min(len - done);
                if n == 0 {
                    return Err(Error::UnmappedLogical(logical + done as u64));
                }
                Self::writable_span(device, m.physical, n)?;
                spans.push((m.physical, done, n));
                done += n;
            }
        }
        Ok(spans)
    }

    /// Write to EVERY copy of a logical range, following the chunk map
    /// exactly as the read path does -- a write that ignored a chunk
    /// boundary would run past the end of one device and into another.
    ///
    /// The first copy alone is right for reading and wrong for writing.
    /// On a `DUP` or `RAID1` chunk it leaves the other copy holding what
    /// was there before, and the two then disagree with no record of
    /// which is current -- a later read may return either. The data path
    /// did exactly that until #71; it is why there is no single-copy
    /// write any more.
    ///
    /// The commit trace confirms this is what the kernel does: both
    /// mirrors of every tree block go out BEFORE the barrier, so a torn
    /// write to one leaves the other and the barrier still orders both
    /// against the superblock.
    ///
    /// # Errors
    ///
    /// Every span is resolved and checked against the device first, so
    /// an unmapped address or a stripe past the end writes nothing. A
    /// device write failure after that propagates, and a partial result
    /// is possible and not cleaned up: some mirrors may hold the new
    /// contents and some the old, which is the same state a power loss
    /// produces and is what the commit ordering exists to survive.
    pub(crate) fn write_logical_all_mirrors(
        device: &Arc<dyn BlockDevice>,
        map: &ChunkMap,
        logical: u64,
        buf: &[u8],
    ) -> Result<()> {
        for (physical, from, n) in Self::mirror_spans(device, map, logical, buf.len())? {
            device.write_at(physical, &buf[from..from + n])?;
        }
        Ok(())
    }

    fn open(device: Arc<dyn BlockRead>, writable: Option<Arc<dyn BlockDevice>>) -> Result<Self> {
        Self::open_pool(device, BTreeMap::new(), writable, None, false)
    }

    /// Open a filesystem that spans several devices.
    ///
    /// Every device of the pool must be given. A chunk stripe names the
    /// disk it lives on, so a missing device is not a partial view —
    /// reads of anything on it return whatever lies at that offset on
    /// whichever disk was consulted instead, which parses and then fails
    /// a checksum against a block it was never meant to be. On a
    /// mirrored pool it may not even fail.
    ///
    /// The devices are identified by the `devid` in each one's own
    /// superblock, not by the order they are passed in.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFeature`] when the set is incomplete, when
    /// two of them are not the same filesystem, or when two claim the
    /// same device id.
    pub fn mount_pool(devices: Vec<Arc<dyn BlockRead>>) -> Result<Self> {
        if devices.is_empty() {
            return Err(Error::UnsupportedFeature(
                "a pool needs at least one device".to_string(),
            ));
        }

        // Each device's own superblock says which device it is and which
        // filesystem it belongs to.
        let mut by_id: BTreeMap<u64, Arc<dyn BlockRead>> = BTreeMap::new();
        let mut fsid: Option<[u8; 16]> = None;
        for dev in devices {
            let (sb, _) = crate::superblock::read_superblock(&*dev)?;

            match fsid {
                None => fsid = Some(sb.fsid),
                Some(seen) if seen != sb.fsid => {
                    return Err(Error::UnsupportedFeature(
                        "these devices belong to different filesystems".to_string(),
                    ))
                }
                Some(_) => {}
            }

            let id = sb.dev_item.devid;
            if by_id.insert(id, dev).is_some() {
                return Err(Error::UnsupportedFeature(format!(
                    "two devices both claim to be device {id}"
                )));
            }
        }

        // Read the filesystem through whichever device holds the
        // superblock's own copy; the rest are reached by devid.
        let first = by_id
            .values()
            .next()
            .expect("at least one device, checked above")
            .clone();
        Self::open_pool(first, by_id, None, None, false)
    }

    /// `known` is a superblock selection already read from `device`'s
    /// underlying disk, which a read-only mount may reuse rather than read
    /// again.
    fn open_pool(
        device: Arc<dyn BlockRead>,
        devices: BTreeMap<u64, Arc<dyn BlockRead>>,
        writable: Option<Arc<dyn BlockDevice>>,
        known: Option<(crate::superblock::Superblock, usize)>,
        allow_unreplayed_log: bool,
    ) -> Result<Self> {
        let (sb, copy) = match known {
            Some(selection) if writable.is_none() => selection,
            _ => crate::superblock::read_superblock(&*device)?,
        };
        // ON THE SELECTION THIS MOUNT USES. Checked on a separate read
        // beforehand, a mirror that failed to read then and read now was
        // mounted writable from the mirror (Greptile on #146).
        if writable.is_some() && copy != 0 {
            return Err(Error::UnsupportedFeature(format!(
                "the primary superblock is damaged or older than copy {copy}; the volume can be \
                 mounted read-only from that copy, and should be checked before it is written"
            )));
        }

        // One device open, and the filesystem says it has more.
        //
        // A chunk stripe names the device it lives on, and with only one
        // device there is nothing to do about a stripe naming another —
        // except refuse. Reading it from the device at hand returns
        // whatever is at that offset on THIS disk: it parses, it fails
        // its checksum against a block it was never meant to be, and on
        // a RAID1 filesystem where the mirror happens to hold the same
        // data it does not even fail. Silently reading one disk of a
        // pool as though it were the whole pool is worse than not
        // opening it.
        if sb.num_devices > 1 && devices.len() as u64 != sb.num_devices {
            return Err(Error::UnsupportedFeature(format!(
                "this filesystem spans {} devices and {} {} given; reading one of them \
                 alone would return the wrong data rather than fail. Open it with \
                 `mount_pool`, giving every device.",
                sb.num_devices,
                devices.len(),
                if devices.len() == 1 { "was" } else { "were" }
            )));
        }

        if sb.has_dirty_log() && !allow_unreplayed_log {
            return Err(Error::DirtyLog);
        }
        // A METADATA-ONLY DUMP HAS NO DATA (#76). `btrfs-image` keeps
        // every tree and none of the extents they point at, so listing and
        // stat would work and every read would return whatever occupies
        // those addresses -- a confident wrong answer. The kernel refuses
        // to mount one, and so does this.
        if sb.is_metadump() {
            return Err(Error::UnsupportedFeature(
                "this image is a metadata-only dump (btrfs-image): its data extents are \
                 absent, so no file in it can be read"
                    .into(),
            ));
        }

        // Step 2: the bootstrap map, enough to reach the chunk tree.
        let boot = ChunkMap::bootstrap(&sb)?;

        // Step 3: walk the chunk tree through it and fold in every chunk.
        let mut map = boot.clone();
        {
            // POOL-AWARE ON THE PRIMARY READ TOO. This was
            // `read_logical`, which reads `device` -- the lowest devid
            // -- at whatever physical offset the mapping names, ignoring
            // the devid it names. The mirror read below was fixed for
            // exactly that and the primary was left behind, so the two
            // halves of one fallback disagreed about which disk a copy
            // is on.
            let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
                Self::read_logical_pool(&device, &devices, &boot, logical, buf)
            };
            let mirrors = |logical: u64| -> Result<usize> { boot.mirrors_at(logical) };
            // POOL-AWARE, BECAUSE A RAID1 COPY IS ON ANOTHER DISK. The
            // single-device form reads every mirror from `device` at the
            // mapping's physical offset, ignoring the devid the mapping
            // names -- so on a two-device pool the fallback read the
            // right offset on the wrong disk and the retry failed the
            // same checksum. Measured: copy 0 of the pool's root damaged
            // on devid 1, copy 1 intact on devid 2, and the mount still
            // returned ChecksumMismatch at copy 0's offset.
            let read_mirror = |logical: u64, mirror: usize, buf: &mut [u8]| -> Result<()> {
                Self::read_logical_pool_mirror(&device, &devices, &boot, mirror, logical, buf)
            };
            let tree = Tree::from_superblock(&sb, &read).with_redundancy(&mirrors, &read_mirror);
            let mut found = Vec::new();
            // Only chunk items, and a chunk item that does not parse or
            // does not fit the sector size fails the mount by name rather
            // than leaving a hole in the map (#85).
            tree.for_each(sb.chunk_root, &mut |key: &DiskKey, data: &[u8]| {
                if let Some(chunk) = crate::chunk::chunk_from_item(key, data, sb.sectorsize)? {
                    found.push(chunk);
                }
                Ok(true)
            })?;
            // The sys_chunk_array in the superblock is a copy of entries
            // that also live in the chunk tree, so folding the tree in
            // re-encounters them. An identical chunk is not a conflict —
            // skip it. A chunk that covers the same address with
            // DIFFERENT contents is a real inconsistency and must not be
            // silently discarded, so it still propagates.
            for chunk in found {
                match map.chunk_for(chunk.logical) {
                    Some(existing) if *existing == chunk => continue,
                    _ => map.insert(chunk)?,
                }
            }
        }

        // Step 4: the root tree names the fs tree, and the csum tree.
        // See `fs_and_csum_tree_roots` for why this is not `.ok()`.
        let (fs_tree_root, csum_tree_root) = {
            // Pool-aware on both halves; see the bootstrap walk above.
            let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
                Self::read_logical_pool(&device, &devices, &map, logical, buf)
            };
            let mirrors = |logical: u64| -> Result<usize> { map.mirrors_at(logical) };
            let read_mirror = |logical: u64, mirror: usize, buf: &mut [u8]| -> Result<()> {
                Self::read_logical_pool_mirror(&device, &devices, &map, mirror, logical, buf)
            };
            let tree = Tree::from_superblock(&sb, &read).with_redundancy(&mirrors, &read_mirror);
            fs_and_csum_tree_roots(&tree, sb.root)?
        };

        let fs = Filesystem {
            device,
            devices,
            writable,
            sb,
            map,
            fs_tree_root,
            csum_tree_root,
            nodes: std::sync::Mutex::new(NodeCache::new(NODE_CACHE_BLOCKS)),
        };
        Ok(fs)
    }

    /// Read a logical range from whichever device of a pool holds it.
    ///
    /// `devices` is empty for a single-device filesystem, and then this
    /// is [`Self::read_logical`]. Otherwise every mapping is answered by
    /// the device its `devid` names — which is the whole difference
    /// between reading a pool and reading one disk of it.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFeature`] when a mapping names a device that
    /// was not given. That is not recoverable by trying another: the
    /// bytes are somewhere this filesystem cannot see.
    /// A tree walker over this filesystem's pool.
    ///
    /// The four lines this replaces —
    ///
    /// ```ignore
    /// let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
    ///     Self::read_logical_pool(&self.device, &self.devices, &self.map, logical, buf)
    /// };
    /// let tree = Tree::from_superblock(&self.sb, &read);
    /// ```
    ///
    /// appeared eleven times, character-identical in ten of them. The
    /// borrow checker is why they were never factored: the closure
    /// borrows three fields and the `Tree` borrows the closure, so a
    /// `fn tree(&self) -> Tree<'_>` cannot return both. Returning the
    /// *reader* instead, and taking the `Tree` from it, splits the two
    /// borrows across two statements, which is all that was needed.
    ///
    /// Callers write:
    ///
    /// ```ignore
    /// let reader = self.pool_reader();
    /// let tree = reader.tree();
    /// ```
    pub(crate) fn pool_reader(&self) -> PoolReader<'_> {
        PoolReader {
            geom: crate::btree::TreeGeometry::from_superblock(&self.sb),
            read: Box::new(move |logical, buf| {
                Self::read_logical_pool(&self.device, &self.devices, &self.map, logical, buf)
            }),
            mirrors: Box::new(move |logical| self.map.mirrors_at(logical)),
            read_mirror: Box::new(move |logical, mirror, buf| {
                Self::read_logical_pool_mirror(
                    &self.device,
                    &self.devices,
                    &self.map,
                    mirror,
                    logical,
                    buf,
                )
            }),
        }
    }

    pub(crate) fn read_logical_pool(
        device: &Arc<dyn BlockRead>,
        devices: &BTreeMap<u64, Arc<dyn BlockRead>>,
        map: &ChunkMap,
        logical: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        Self::read_logical_pool_mirror(device, devices, map, 0, logical, buf)
    }

    /// [`Filesystem::read_logical_pool`], from copy `mirror`.
    ///
    /// On a RAID1 pool the second copy is on a different device, which
    /// is the case this exists for: the mapping names its own device,
    /// so following the mirror follows the disk with it.
    pub(crate) fn read_logical_pool_mirror(
        device: &Arc<dyn BlockRead>,
        devices: &BTreeMap<u64, Arc<dyn BlockRead>>,
        map: &ChunkMap,
        mirror: usize,
        logical: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        if devices.is_empty() {
            return Self::read_logical_on_mirror(device, map, u64::MAX, mirror, logical, buf);
        }
        let mut done = 0usize;
        while done < buf.len() {
            let m = map.map_mirror(logical + done as u64, mirror)?;
            let n = (m.len as usize).min(buf.len() - done);
            if n == 0 {
                return Err(Error::UnmappedLogical(logical + done as u64));
            }
            let dev = devices.get(&m.devid).ok_or_else(|| {
                Error::UnsupportedFeature(format!(
                    "the range at {} lives on device {}, which was not given",
                    logical + done as u64,
                    m.devid
                ))
            })?;
            dev.read_at(m.physical, &mut buf[done..done + n])?;
            done += n;
        }
        Ok(())
    }

    /// Read a logical range, refusing anything that lives on a device
    /// other than `devid`.
    ///
    /// A [`Mapping`] names the device its physical offset is on, and
    /// with one device open there is nothing to do about a mapping that
    /// names another — except say so. Reading it from the device at hand
    /// returns whatever happens to be at that offset, which parses,
    /// checksums against the wrong block, and is silently the wrong
    /// data.
    ///
    /// `u64::MAX` means "do not check", used where the caller has
    /// already established there is only one device.
    ///
    /// # THE TWO CONVENIENCE WRAPPERS OVER THIS ARE GONE
    ///
    /// `read_logical` and `read_logical_on` used to wrap this with
    /// `devid = u64::MAX` and `mirror = 0`, and three sites in the
    /// mount path picked them for a POOL -- reading the right offset on
    /// the lowest devid whatever devid the mapping named. Changing the
    /// three call sites would have left the wrappers there for the
    /// fourth, so they are deleted instead: on a pool the only
    /// spellings available now are the `_pool` pair, which resolve the
    /// devid. Anything that genuinely has one device passes that
    /// device's own devid here and gets an error rather than the wrong
    /// bytes.
    ///
    /// From copy `mirror`.
    ///
    /// Mirror 0 is the copy every other read path uses. The rest are
    /// the copies DUP and RAID1 keep, and the reason the read path can
    /// survive one damaged block: see [`crate::btree::Tree::read_block`],
    /// which is where a failed verification turns into a second attempt.
    pub(crate) fn read_logical_on_mirror(
        device: &Arc<dyn BlockRead>,
        map: &ChunkMap,
        devid: u64,
        mirror: usize,
        logical: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        let mut done = 0usize;
        while done < buf.len() {
            let m = map.map_mirror(logical + done as u64, mirror)?;
            if devid != u64::MAX && m.devid != devid {
                return Err(Error::UnsupportedFeature(format!(
                    "the range at {} lives on device {} and this filesystem was opened \
                     with device {devid} alone; reading it from the wrong device would \
                     return the wrong data rather than fail",
                    logical + done as u64,
                    m.devid
                )));
            }
            // A read may span two chunks, so never take more than the
            // mapping says is contiguous.
            let n = (m.len as usize).min(buf.len() - done);
            if n == 0 {
                return Err(Error::UnmappedLogical(logical + done as u64));
            }
            device.read_at(m.physical, &mut buf[done..done + n])?;
            done += n;
        }
        Ok(())
    }

    /// A handle over the same device, reading a different tree.
    ///
    /// Used by [`Filesystem::open_subvolume`]. The device, superblock
    /// and chunk map are shared — they describe the volume rather than
    /// any one tree — and only the root and the items loaded from it
    /// differ.
    ///
    /// The write capability is deliberately not carried across; see the
    /// note on `open_subvolume`.
    pub(crate) fn reroot(&self, fs_tree_root: u64) -> Result<Self> {
        let fs = Filesystem {
            device: self.device.clone(),
            devices: self.devices.clone(),
            writable: None,
            sb: self.sb.clone(),
            map: self.map.clone(),
            fs_tree_root,
            // A subvolume is another tree on the same volume, so its
            // data is checksummed by the same csum tree.
            csum_tree_root: self.csum_tree_root,
            nodes: std::sync::Mutex::new(NodeCache::new(NODE_CACHE_BLOCKS)),
        };
        Ok(fs)
    }

    /// Run `f` over this mount's fs tree, its blocks read through the node
    /// cache. The pool, mirror fallback and verification are as every other
    /// tree walk here has them.
    fn with_fs_tree<R>(&self, f: impl FnOnce(&Tree, u64) -> Result<R>) -> Result<R> {
        let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
            Self::read_logical_pool(&self.device, &self.devices, &self.map, logical, buf)
        };
        let mirrors = |logical: u64| -> Result<usize> { self.map.mirrors_at(logical) };
        let read_mirror = |logical: u64, mirror: usize, buf: &mut [u8]| -> Result<()> {
            Self::read_logical_pool_mirror(
                &self.device,
                &self.devices,
                &self.map,
                mirror,
                logical,
                buf,
            )
        };
        let get = |logical: u64| self.nodes.lock().expect("node cache poisoned").get(logical);
        let put = |logical: u64, block: &crate::btree::TreeBlock| {
            self.nodes
                .lock()
                .expect("node cache poisoned")
                .put(logical, block)
        };
        let tree = Tree::new(TreeGeometry::from_superblock(&self.sb), &read)
            .with_redundancy(&mirrors, &read_mirror)
            .with_cache(&get, &put);
        f(&tree, self.fs_tree_root)
    }

    /// The fs tree item filed under exactly `key`.
    fn item(&self, key: (u64, u8, u64)) -> Result<Option<Vec<u8>>> {
        let key = DiskKey {
            objectid: key.0,
            key_type: key.1,
            offset: key.2,
        };
        self.with_fs_tree(|tree, root| Ok(tree.search(root, &key)?.map(|item| item.data)))
    }

    /// Every fs tree item under `objectid` and `key_type`, in key order.
    #[allow(clippy::type_complexity)]
    fn item_run(&self, objectid: u64, key_type: u8) -> Result<Vec<((u64, u8, u64), Vec<u8>)>> {
        self.with_fs_tree(|tree, root| {
            Ok(tree
                .find_all(root, objectid, key_type)?
                .into_iter()
                .map(|item| {
                    (
                        (item.key.objectid, item.key.key_type, item.key.offset),
                        item.data,
                    )
                })
                .collect())
        })
    }

    /// Forget every cached tree block. A commit writes blocks at addresses
    /// the tree may since have freed and reused.
    pub(crate) fn forget_tree_blocks(&self) {
        self.nodes.lock().expect("node cache poisoned").clear();
    }

    /// The parsed superblock.
    /// Read one tree block by its logical address.
    ///
    /// A writer needs to look at a specific block rather than iterate
    /// items: to find what is above a leaf, to check what is at an
    /// address before placing something there.
    ///
    /// # Errors
    ///
    /// Propagates the read, and the block's own verification — an
    /// address holding something that is not a tree block of this
    /// filesystem is an error rather than an empty result.
    pub fn read_tree_block(&self, logical: u64) -> Result<crate::btree::TreeBlock> {
        let reader = self.pool_reader();
        reader.tree().read_block(logical)
    }

    /// The chunk map — how logical addresses become physical ones.
    ///
    /// Exposed because a WRITER has to reason about placement in a way a
    /// reader does not: how many copies an address has, and where each
    /// one lands.
    pub fn chunk_map(&self) -> &ChunkMap {
        &self.map
    }

    pub fn superblock(&self) -> &Superblock {
        &self.sb
    }

    /// Every item in the root tree.
    ///
    /// The root tree is the index of trees: one `ROOT_ITEM` per
    /// subvolume saying where its tree lives, and reference items
    /// naming them. This hands the raw items back so a caller can see
    /// what is actually there rather than only what is understood.
    ///
    /// # Errors
    ///
    /// As the B-tree walk.
    pub fn root_tree_items(&self) -> Result<Vec<RootTreeItem>> {
        let reader = self.pool_reader();
        let tree = reader.tree();
        let mut out = Vec::new();
        tree.for_each(self.sb.root, &mut |key: &DiskKey, data: &[u8]| {
            out.push((key.objectid, key.key_type, key.offset, data.to_vec()));
            Ok(true)
        })?;
        Ok(out)
    }

    /// Read one inode by objectid.
    pub fn read_inode(&self, ino: u64) -> Result<Inode> {
        let data = self
            .item((ino, INODE_ITEM_KEY, 0))?
            .ok_or(Error::NotFound)?;
        Inode::parse(&data, ino)
    }

    /// The root directory's inode.
    pub fn root_inode(&self) -> Result<Inode> {
        self.read_inode(FIRST_FREE_OBJECTID)
    }

    /// List a directory's entries.
    ///
    /// Uses `DIR_INDEX` rather than `DIR_ITEM`: the index is ordered and
    /// holds exactly one entry per key, while `DIR_ITEM` is hashed and
    /// packs colliding names into a single value. Both describe the same
    /// set; the index is simply the one meant for iteration.
    ///
    /// `.` and `..` are never returned, matching the sibling XFS driver
    /// so a caller does not have to special-case per filesystem.
    pub fn read_dir(&self, ino: u64) -> Result<Vec<DirEntry>> {
        let inode = self.read_inode(ino)?;
        if !inode.is_dir() {
            return Err(Error::NotADirectory);
        }
        let mut out = Vec::new();
        for ((objectid, key_type, _), data) in &self.item_run(ino, DIR_INDEX_KEY)? {
            if *objectid != ino || *key_type != DIR_INDEX_KEY {
                break;
            }
            for e in dir::parse_dir_items(data)? {
                if e.name != b"." && e.name != b".." {
                    out.push(e);
                }
            }
        }
        Ok(out)
    }

    /// Look up one name within a directory.
    ///
    /// # Why this is not `read_dir().find(..)`
    ///
    /// A name is filed under `(dir, DIR_ITEM, name_hash(name))`, so the
    /// item holding it is fetched by key, and the scan that remains is over
    /// the records packed into that one item -- names whose hashes collide.
    /// Listing the directory to find one name made every path component cost
    /// the size of its directory: a lookup in a 20,000-entry directory
    /// materialised all 20,000 entries (#64).
    pub fn lookup(&self, dir_ino: u64, name: &[u8]) -> Result<Inode> {
        let hit = self.lookup_entry(dir_ino, name)?;

        // A subvolume is a directory entry whose location names a tree
        // rather than an inode, so there is nothing in THIS tree to
        // return. Saying so is the point: reading the entry's objectid
        // as an inode number finds an unrelated inode of the same
        // number, or nothing, and `NotFound` for a name that is plainly
        // there sends the reader looking in the wrong place entirely.
        if !hit.is_inode() {
            return Err(Error::UnsupportedFeature(format!(
                "{:?} names subvolume {} rather than an inode in this tree — resolve \
                 the path with `resolve_path`, which crosses into it",
                String::from_utf8_lossy(name),
                hit.ino
            )));
        }
        self.read_inode(hit.ino)
    }

    /// The directory entry `name` in `dir_ino`, whether it names an inode
    /// or a subvolume.
    pub(crate) fn lookup_entry(&self, dir_ino: u64, name: &[u8]) -> Result<DirEntry> {
        if !self.read_inode(dir_ino)?.is_dir() {
            return Err(Error::NotADirectory);
        }
        // Not stored as entries, and `read_dir` never returned them.
        if name == b"." || name == b".." {
            return Err(Error::NotFound);
        }
        let data = self
            .item((dir_ino, dir::DIR_ITEM_KEY, dir::name_hash(name)))?
            .ok_or(Error::NotFound)?;
        dir::parse_dir_items(&data)?
            .into_iter()
            .find(|e| e.name == name)
            .ok_or(Error::NotFound)
    }

    /// Every extended attribute on an inode, in the order the tree
    /// holds them.
    ///
    /// That order is by name hash, not alphabetical and not the order
    /// the attributes were set — Btrfs files an attribute under
    /// `(ino, 24, name_hash(name))`, so the sequence is whatever the
    /// hash function produced. A caller wanting a stable presentation
    /// order should sort; this reports what is there.
    ///
    /// Several names can share one key, and then their records are
    /// packed into a single item. Every record in every item is
    /// returned, which is the whole reason this is not a lookup.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if the inode does not exist, or
    /// [`Error::BadSuperblock`] if an item is malformed.
    pub fn list_xattrs(&self, ino: u64) -> Result<Vec<XattrEntry>> {
        // Confirms the inode exists, so an attribute list for a number
        // that names nothing is a refusal rather than an empty list —
        // which a caller could not tell from a file with no attributes.
        self.read_inode(ino)?;
        let mut out = Vec::new();
        for ((objectid, key_type, _), data) in &self.item_run(ino, XATTR_ITEM_KEY)? {
            if *objectid != ino || *key_type != XATTR_ITEM_KEY {
                break;
            }
            out.extend(xattr::parse_xattr_items(data)?);
        }
        Ok(out)
    }

    /// One extended attribute's value, by fully-qualified name
    /// (`user.colour`, not `colour`).
    ///
    /// `Ok(None)` means the attribute is not set. That is distinct from
    /// `Ok(Some(vec![]))`, which means it is set to a zero-length value
    /// — a real thing to store, and something a caller may act on.
    ///
    /// # Why this is not `list_xattrs().find(..)`
    ///
    /// The name's hash *is* the key, so the item holding it can be
    /// fetched directly instead of walking every attribute on the
    /// inode. The scan that remains is over the handful of records
    /// inside that one item, which is where a colliding name would be.
    ///
    /// # Errors
    ///
    /// As [`list_xattrs`](Self::list_xattrs).
    pub fn get_xattr(&self, ino: u64, name: &[u8]) -> Result<Option<Vec<u8>>> {
        self.read_inode(ino)?;
        let Some(data) = self.item((ino, XATTR_ITEM_KEY, dir::name_hash(name)))? else {
            return Ok(None);
        };
        Ok(xattr::parse_xattr_items(&data)?
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| e.value))
    }

    /// Resolve an absolute path to its inode in this tree.
    ///
    /// A path into a subvolume is refused at the boundary, because the
    /// inode would mean nothing without its tree; [`resolve_path`]
    /// crosses it and returns both.
    ///
    /// Symbolic links are not followed, so link loops remain the
    /// caller's policy rather than a surprise from this function.
    ///
    /// [`resolve_path`]: Filesystem::resolve_path
    pub fn lookup_path(&self, path: &str) -> Result<Inode> {
        self.lookup_path_bytes(path.as_bytes())
    }

    /// Resolve a `/`-separated path given as bytes.
    ///
    /// THIS IS THE REAL ONE, and the `&str` wrapper above is the
    /// convenience. Btrfs directory entry names are raw bytes and the
    /// format has no field that could say what encoding they are in, so
    /// a name is not text until somebody decides what to read it as,
    /// and this crate never decides. Components are compared byte for
    /// byte against the on-disk entry, so a name handed out by a listing
    /// always resolves when handed back (#214).
    pub fn lookup_path_bytes(&self, path: &[u8]) -> Result<Inode> {
        let mut inode = self.root_inode()?;
        for component in path
            .split(|&b| b == b'/')
            .filter(|c| !c.is_empty() && *c != b".")
        {
            if component == b".." {
                return Err(Error::UnsupportedFeature(
                    "`..` in a path is not resolved by lookup_path".into(),
                ));
            }
            if !inode.is_dir() {
                return Err(Error::NotADirectory);
            }
            inode = self.lookup(inode.ino, component)?;
        }
        Ok(inode)
    }

    /// Refuse a reference whose window leaves the extent the extent tree
    /// records for it (#188).
    ///
    /// `offset` and `len` are the reference's window within the extent that
    /// starts at `disk_bytenr`. The extent tree's item for that address
    /// carries the extent's length in its key, which is the one bound that
    /// does not come from the item being checked.
    ///
    /// # Errors
    ///
    /// [`Error::BadSuperblock`] when the window leaves the extent, and
    /// whatever the extent-tree lookup returns — including an extent with
    /// no item at all, which is a reference to something nothing records.
    fn refuse_window_outside_extent(
        &self,
        ino: u64,
        disk_bytenr: u64,
        offset: u64,
        len: u64,
    ) -> Result<()> {
        // A reference with no address behind it: a hole or inline data,
        // which the caller has already dealt with.
        if disk_bytenr == 0 {
            return Ok(());
        }
        let (_, extent_len, _) = self.extent_item(disk_bytenr)?;
        let start = disk_bytenr.checked_add(offset).ok_or_else(|| {
            Error::BadSuperblock(format!(
                "inode {ino}: an extent item at {disk_bytenr} has an offset of {offset}, \
                 which is past the end of the address space"
            ))
        })?;
        if !crate::write::window_inside_extent(start, len, disk_bytenr, extent_len) {
            return Err(Error::BadSuperblock(format!(
                "inode {ino}: an extent item covers [{start}, +{len}), which is outside the \
                 extent at {disk_bytenr} that the extent tree records as {extent_len} bytes"
            )));
        }
        Ok(())
    }

    /// Decode one `EXTENT_DATA` item into the piece of file it describes.
    fn decode_extent<'a>(&self, data: &'a [u8], ino: u64) -> Result<Piece<'a>> {
        if data.len() < file_extent::TYPE + 1 {
            return Err(Error::BadSuperblock(format!(
                "inode {ino}: extent item is {} bytes, too short to hold a type",
                data.len()
            )));
        }
        let _generation = le64(data, file_extent::GENERATION);
        let ram_bytes = le64(data, file_extent::RAM_BYTES);
        let compression = data[file_extent::COMPRESSION];
        let encryption = data[file_extent::ENCRYPTION];
        let other = u16::from_le_bytes(
            data[file_extent::OTHER_ENCODING..file_extent::OTHER_ENCODING + 2]
                .try_into()
                .expect("2 bytes"),
        );
        let kind = data[file_extent::TYPE];

        let algo = Compression::from_byte(compression)
            .map_err(|e| Error::UnsupportedFeature(format!("inode {ino}: {e}")))?;
        if encryption != 0 || other != 0 {
            return Err(Error::UnsupportedFeature(format!(
                "inode {ino}: extent is encoded (encryption {encryption}, other {other})"
            )));
        }

        match kind {
            EXTENT_INLINE => {
                let end = data.len();
                let start = file_extent::INLINE_DATA.min(end);
                let raw = &data[start..end];
                // An inline extent may be compressed too, and there is no
                // offset to apply: the item holds the whole thing.
                Ok(Piece::Inline(if algo.is_compressed() {
                    // An inline extent is the whole of a small file, so it
                    // is the file's last extent by definition (#189).
                    std::borrow::Cow::Owned(compression::decompress(
                        algo,
                        raw,
                        ram_bytes as usize,
                        self.sb.sectorsize as usize,
                        compression::ShortDecode::Pad,
                    )?)
                } else {
                    std::borrow::Cow::Borrowed(raw)
                }))
            }
            EXTENT_REGULAR | EXTENT_PREALLOC => {
                if data.len() < file_extent::REGULAR_SIZE {
                    return Err(Error::BadSuperblock(format!(
                        "inode {ino}: non-inline extent item is {} bytes, need {}",
                        data.len(),
                        file_extent::REGULAR_SIZE
                    )));
                }
                let disk_bytenr = le64(data, file_extent::DISK_BYTENR);
                let offset = le64(data, file_extent::OFFSET);
                let num_bytes = le64(data, file_extent::NUM_BYTES);

                // EVERY LENGTH AND ADDRESS IS WHOLE SECTORS, which is what
                // the kernel's tree checker (`check_extent_data_item`)
                // requires of all five fields, holes included. It narrows
                // the values a crafted item can use; it is not a bound on
                // where the window points (#89).
                let sector = u64::from(self.sb.sectorsize.max(1));
                for (field, value) in [
                    ("ram_bytes", ram_bytes),
                    ("disk_bytenr", disk_bytenr),
                    ("disk_num_bytes", le64(data, file_extent::DISK_NUM_BYTES)),
                    ("offset", offset),
                    ("num_bytes", num_bytes),
                ] {
                    if value % sector != 0 {
                        return Err(Error::BadSuperblock(format!(
                            "inode {ino}: extent item {field} {value} is not a multiple of \
                             the {sector}-byte sector"
                        )));
                    }
                }

                // disk_bytenr == 0 is a hole. A preallocated extent has
                // blocks reserved but never written, and returning them
                // would disclose whatever previously occupied the space.
                if disk_bytenr == 0 {
                    return Ok(Piece::Hole);
                }
                if kind == EXTENT_PREALLOC {
                    return Ok(Piece::Preallocated {
                        logical: disk_bytenr,
                        len: num_bytes,
                    });
                }
                // THE ITEM'S WINDOW IS INSIDE THE EXTENT IT NAMES.
                //
                // `offset` says where in the extent this item's data
                // starts and `num_bytes` how much of it the item
                // covers, so together they cannot exceed the extent's
                // own length. This crate's rule, not the kernel tree
                // checker's, and a weak one: `ram_bytes` comes from the
                // same item, so a crafted item passes it by raising
                // `ram_bytes` too (#89). It still refuses the plain
                // inconsistency. The bound that holds is the extent
                // tree's own record of the extent's length, from another
                // tree entirely; the write path checks against that
                // before writing (`plan_nodatacow_write`), because a
                // window moved outside its extent there writes over a
                // tree block or another file. A read through such an
                // item returns bytes from outside the extent.
                //
                // BEFORE THE COMPRESSED BRANCH, NOT AFTER IT (#73). The
                // rule is a property of the item, not of how its bytes
                // are stored, and for a compressed extent `offset` is an
                // index into the DECODED buffer that never meets the chunk
                // map: unchecked, one le64 overflowed that index into a
                // panic in any build with overflow checks on.
                let window_end = offset
                    .checked_add(num_bytes)
                    .filter(|end| *end <= ram_bytes);
                if window_end.is_none() {
                    return Err(Error::BadSuperblock(format!(
                        "inode {ino}: an extent item covers [{offset}, +{num_bytes}) of an \
                         extent that is {ram_bytes} bytes long"
                    )));
                }
                if algo.is_compressed() {
                    // `disk_len` is the buffer the compressed bytes are
                    // read into, and it is a raw le64. Btrfs never
                    // writes a compressed extent larger than the unit it
                    // compresses in -- see `compression::MAX_COMPRESSED`.
                    let disk_len = le64(data, file_extent::DISK_NUM_BYTES);
                    if disk_len > compression::MAX_COMPRESSED {
                        return Err(Error::BadSuperblock(format!(
                            "inode {ino}: compressed extent occupies {disk_len} bytes on \
                             disk, more than the {} a compressed extent can",
                            compression::MAX_COMPRESSED
                        )));
                    }
                    // The compressed run is read whole from `disk_bytenr`,
                    // so that is the range which has to be inside the
                    // extent; `offset` indexes the decoded bytes and is
                    // bounded by `ram_bytes` above (#188).
                    self.refuse_window_outside_extent(ino, disk_bytenr, 0, disk_len)?;
                    return Ok(Piece::Compressed {
                        logical: disk_bytenr,
                        disk_len,
                        ram_len: ram_bytes,
                        offset,
                        len: num_bytes,
                        algo,
                    });
                }
                // THE WINDOW IS INSIDE THE EXTENT THE EXTENT TREE RECORDS
                // (#188). The check above bounds it by `ram_bytes`, which
                // is a field of this same item, so an item that raises both
                // passes — and a read then answered with whatever occupies
                // the addresses past the extent: another file's data, or a
                // tree block, with no error to tell a caller. The write
                // path has checked against the extent tree since #156; the
                // length there is the extent's own record, from a tree this
                // item cannot forge.
                self.refuse_window_outside_extent(ino, disk_bytenr, offset, num_bytes)?;
                Ok(Piece::Regular {
                    // Both halves are raw le64s. In release, where this
                    // crate ships with overflow-checks off, the sum
                    // wrapped to a small logical address that then
                    // mapped into an unrelated chunk -- a silent
                    // mistranslation, which is the failure this driver
                    // exists to avoid.
                    logical: disk_bytenr.checked_add(offset).ok_or_else(|| {
                        Error::BadSuperblock(format!(
                            "inode {ino}: extent at {disk_bytenr} plus offset {offset} \
                             leaves the address space"
                        ))
                    })?,
                    len: num_bytes,
                })
            }
            other => Err(Error::BadSuperblock(format!(
                "inode {ino}: extent type {other} is not a defined value (ram_bytes {ram_bytes})"
            ))),
        }
    }

    /// One extent of a file, as the write planner needs to see it.
    ///
    /// The read path consumes `Piece` and copies immediately; a writer
    /// has to decide whether a range may be written at all before
    /// touching any of it, which needs the pieces as a list with their
    /// file offsets attached.
    pub(crate) fn file_extents(&self, ino: u64) -> Result<Vec<FileExtent>> {
        let mut out = Vec::new();
        for ((objectid, key_type, offset), data) in &self.item_run(ino, EXTENT_DATA_KEY)? {
            if *objectid != ino || *key_type != EXTENT_DATA_KEY {
                break;
            }
            let start = *offset;
            match self.decode_extent(data, ino)? {
                // Inline data lives in the item, so there is no block to
                // overwrite, and a preallocated extent's blocks read as
                // zeros until the item is changed to say otherwise. Both
                // are reported with no logical address, and the planner
                // refuses them by name. A hole has nothing behind it and
                // is not reported.
                Piece::Inline(bytes) => out.push(FileExtent {
                    start,
                    len: bytes.len() as u64,
                    logical: None,
                    extent_start: 0,
                    compressed: false,
                }),
                Piece::Preallocated { logical, len } => out.push(FileExtent {
                    start,
                    len,
                    logical: None,
                    extent_start: logical,
                    compressed: false,
                }),
                Piece::Hole => {}
                Piece::Regular { logical, len } => out.push(FileExtent {
                    start,
                    len,
                    logical: Some(logical),
                    // `logical` already has the reference's offset folded
                    // in; the extent item is keyed by the run's start.
                    extent_start: le64(data, file_extent::DISK_BYTENR),
                    compressed: false,
                }),
                Piece::Compressed { len, .. } => out.push(FileExtent {
                    start,
                    len,
                    logical: None,
                    extent_start: le64(data, file_extent::DISK_BYTENR),
                    compressed: true,
                }),
            }
        }
        Ok(out)
    }

    /// Read a whole file.
    ///
    /// Materialises the file in memory, so it is bounded by the size of
    /// the filesystem and by [`MAX_WHOLE_FILE_READ`], and the allocation
    /// is fallible: a size no allocator can satisfy is an error rather
    /// than an abort. Reading part of a larger file is what
    /// [`Filesystem::read_at`] is for.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFeature`] for a file too large to read whole:
    /// the request is valid and this call cannot serve it, which is not
    /// a device failure ([`Error::Io`]) and not proof of corruption.
    pub fn read_file(&self, ino: u64) -> Result<Vec<u8>> {
        let inode = self.read_inode(ino)?;
        if !inode.is_regular_file() && !inode.is_symlink() {
            return Err(Error::NotAFile);
        }
        if inode.size > self.sb.total_bytes {
            return Err(Error::BadSuperblock(format!(
                "inode {ino} says it is {} bytes, more than the {} the filesystem holds",
                inode.size, self.sb.total_bytes
            )));
        }
        // BOUNDED, AND THEN FALLIBLY. `inode.size` is a raw `le64`, and
        // `total_bytes` bounds it only as tightly as the volume is small:
        // an 8 TB volume admits an 8 TB `Vec`. An allocation that cannot
        // be satisfied aborts the process, which `capi::guard` cannot turn
        // into an errno (#80). A sparse file can truly be that large, so
        // the extents cannot bound it either. What can be refused is the
        // read: past `MAX_WHOLE_FILE_READ` by a fixed ceiling, because an
        // overcommitting allocator grants reservations it cannot back,
        // and below it by the reservation, for one that does not.
        //
        // `UnsupportedFeature`, not `Io`: nothing failed on the device,
        // and a caller matching variants must not read this as one.
        let too_large = || {
            Error::UnsupportedFeature(format!(
                "inode {ino} is {} bytes, more than a whole-file read will hold in memory; \
                 read it in pieces with read_at",
                inode.size
            ))
        };
        if inode.size > MAX_WHOLE_FILE_READ {
            return Err(too_large());
        }
        let len = usize::try_from(inode.size).map_err(|_| too_large())?;
        let mut out = Vec::new();
        out.try_reserve_exact(len).map_err(|_| too_large())?;
        out.resize(len, 0);
        self.read_range(&inode, 0, &mut out)?;
        Ok(out)
    }

    /// Read data at a logical address and check it against the csum
    /// tree before handing it back.
    ///
    /// Data checksums cover whole sectors, so a read that starts or
    /// ends mid-sector cannot be verified as it stands: there is
    /// nothing to compare a fragment against. The read is widened to
    /// sector boundaries, every sector the tree names is checked, and
    /// the caller's slice is filled from the middle of the result. The
    /// widening costs at most two extra sectors per extent and is what
    /// makes the check cover the bytes actually returned rather than
    /// only the aligned ones.
    ///
    /// A sector the tree does not name is passed through unchecked. See
    /// [`crate::csum`] for why absence is not a failure.
    ///
    /// # Errors
    ///
    /// [`Error::ChecksumMismatch`] naming the logical address of the
    /// first sector whose digest does not match — the address, not the
    /// file offset, because that is what a caller comparing against
    /// `btrfs inspect-internal` or the kernel's own message will see.
    fn read_data_verified(&self, logical: u64, dst: &mut [u8], verify: bool) -> Result<()> {
        if !verify || dst.is_empty() {
            return Self::read_logical_pool(&self.device, &self.devices, &self.map, logical, dst);
        }
        let Some(csum_root) = self.csum_tree_root else {
            return Self::read_logical_pool(&self.device, &self.devices, &self.map, logical, dst);
        };
        let sector = u64::from(self.sb.sectorsize);
        if sector == 0 {
            return Self::read_logical_pool(&self.device, &self.devices, &self.map, logical, dst);
        }

        let (first, span) = sector_aligned_span(logical, dst.len() as u64, sector)
            .ok_or(Error::UnmappedLogical(logical))?;

        let mut whole = vec![0u8; span as usize];
        Self::read_logical_pool(&self.device, &self.devices, &self.map, first, &mut whole)?;

        let digests = {
            let reader = self.pool_reader();
            let tree = reader.tree();
            crate::csum::digests_for_range(
                &tree,
                csum_root,
                self.sb.csum_type.digest_len(),
                sector,
                first,
                span,
            )?
        };

        for (i, chunk) in whole.chunks_exact(sector as usize).enumerate() {
            let at = first + i as u64 * sector;
            let Some(expected) = digests.get(&at) else {
                continue;
            };
            if !self.sb.csum_type.verify(chunk, expected) {
                return Err(Error::ChecksumMismatch {
                    what: "a data extent",
                    offset: at,
                });
            }
        }

        let skip = (logical - first) as usize;
        dst.copy_from_slice(&whole[skip..skip + dst.len()]);
        Ok(())
    }

    /// Fill `buf` with the file's bytes from `from` onwards.
    ///
    /// `buf` is zeroed first, so a hole and a preallocated extent need
    /// no special case: they are the absence of a copy.
    ///
    /// Only the extents that overlap the window are read. `read_at`
    /// used to call `read_file` and slice the result, which meant
    /// serving a 4 KiB read of a large file by materialising the whole
    /// of it -- and `inode.size` is a raw `le64`, so "the whole of it"
    /// was whatever the image claimed.
    fn read_range(&self, inode: &Inode, from: u64, buf: &mut [u8]) -> Result<()> {
        buf.fill(0);
        if buf.is_empty() {
            return Ok(());
        }
        let ino = inode.ino;
        // A file the filesystem never checksummed has nothing to check
        // against. Asked per file rather than per sector because it is
        // the answer for every sector of this file, and because a
        // `NODATASUM` file would otherwise pay a csum-tree descent per
        // extent to be told each time that there is nothing there.
        let verify =
            inode.flags & (crate::write::INODE_NODATASUM | crate::write::INODE_NODATACOW) == 0;
        let want_end = from.saturating_add(buf.len() as u64).min(inode.size);

        for ((objectid, key_type, offset), data) in &self.item_run(ino, EXTENT_DATA_KEY)? {
            if *objectid != ino || *key_type != EXTENT_DATA_KEY {
                break;
            }
            let at = *offset;
            if at >= want_end {
                break;
            }
            let piece = self.decode_extent(data, ino)?;
            let extent_len = match &piece {
                Piece::Inline(bytes) => bytes.len() as u64,
                Piece::Hole | Piece::Preallocated { .. } => continue,
                Piece::Regular { len, .. } | Piece::Compressed { len, .. } => *len,
            };
            // Where this extent and the window overlap, in file offsets.
            let start = at.max(from);
            let end = at.saturating_add(extent_len).min(want_end);
            if end <= start {
                continue;
            }
            let take = (end - start) as usize;
            let skip = (start - at) as usize;
            let dst = &mut buf[(start - from) as usize..(end - from) as usize];

            match piece {
                Piece::Inline(bytes) => {
                    let src = bytes
                        .get(skip..skip + take)
                        .ok_or_else(|| short_extent(ino, "inline"))?;
                    dst.copy_from_slice(src);
                }
                Piece::Hole | Piece::Preallocated { .. } => unreachable!("handled above"),
                Piece::Regular { logical, .. } => {
                    let at_logical = logical
                        .checked_add(skip as u64)
                        .ok_or_else(|| short_extent(ino, "regular"))?;
                    self.read_data_verified(at_logical, dst, verify)?;
                }
                Piece::Compressed {
                    logical,
                    disk_len,
                    ram_len,
                    offset: within,
                    algo,
                    ..
                } => {
                    let mut packed = vec![0u8; disk_len as usize];
                    // A compressed extent's checksums cover the bytes
                    // as they are on disk, not the decoded ones, so
                    // this is the read to verify — and it is the whole
                    // extent, so it is sector-aligned already.
                    self.read_data_verified(logical, &mut packed, verify)?;
                    // A DECODE MAY ONLY COME UP SHORT ON THE LAST EXTENT
                    // (#189), where the tail of the final sector holds
                    // nothing. Anywhere else the stream is truncated, and
                    // padding it would answer with zeros a caller cannot
                    // tell from data.
                    let short = if at.saturating_add(ram_len) >= inode.size {
                        compression::ShortDecode::Pad
                    } else {
                        compression::ShortDecode::Refuse
                    };
                    let decoded = compression::decompress(
                        algo,
                        &packed,
                        ram_len as usize,
                        self.sb.sectorsize as usize,
                        short,
                    )?;
                    // `within` indexes the decoded bytes, which is the
                    // whole reason this is not a Regular read.
                    let src = usize::try_from(within)
                        .ok()
                        .and_then(|w| w.checked_add(skip))
                        .and_then(|at| Some(at..at.checked_add(take)?))
                        .and_then(|range| decoded.get(range))
                        .ok_or_else(|| short_extent(ino, "compressed"))?;
                    dst.copy_from_slice(src);
                }
            }
        }
        Ok(())
    }

    /// Read part of a file.
    ///
    /// Returns the number of bytes read, short only at end of file.
    pub fn read_at(&self, ino: u64, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let inode = self.read_inode(ino)?;
        // Kept from when this went through `read_file`: reading a
        // directory is EISDIR, not a short read of nothing.
        if !inode.is_regular_file() && !inode.is_symlink() {
            return Err(Error::NotAFile);
        }
        if offset >= inode.size {
            return Ok(0);
        }
        let n = buf.len().min((inode.size - offset) as usize);
        self.read_range(&inode, offset, &mut buf[..n])?;
        Ok(n)
    }

    /// Resolve a symbolic link's target.
    pub fn read_link(&self, ino: u64) -> Result<Vec<u8>> {
        let inode = self.read_inode(ino)?;
        if !inode.is_symlink() {
            return Err(Error::NotAFile);
        }
        // A link's target is a path, and no path is longer than
        // PATH_MAX. Without this the inode's raw `size` reached
        // `read_file`'s allocation from `fs_btrfs_readlink`.
        if inode.size > MAX_SYMLINK_TARGET {
            return Err(Error::BadSuperblock(format!(
                "inode {ino}: symlink target of {} bytes is longer than any path",
                inode.size
            )));
        }
        self.read_file(ino)
    }

    /// List a directory by path, crossing into subvolumes on the way.
    pub fn list_path(&self, path: &str) -> Result<Vec<DirEntry>> {
        self.list_path_bytes(path.as_bytes())
    }

    /// [`list_path`] with the path given as bytes (#214).
    ///
    /// [`list_path`]: Self::list_path
    pub fn list_path_bytes(&self, path: &[u8]) -> Result<Vec<DirEntry>> {
        let target = self.resolve_path_bytes(path)?;
        if target.inode.ino == crate::subvol::EMPTY_SUBVOL_DIR_OBJECTID {
            return Ok(Vec::new());
        }
        target.fs(self).read_dir(target.inode.ino)
    }

    /// Read a whole file by path, crossing into subvolumes on the way.
    pub fn read_path(&self, path: &str) -> Result<Vec<u8>> {
        let target = self.resolve_path(path)?;
        target.fs(self).read_file(target.inode.ino)
    }
}

/// A tree walker bound to one filesystem's pool.
///
/// Holds the read closure so a [`crate::btree::Tree`] can borrow it —
/// see [`Filesystem::pool_reader`] for why the two cannot be one value.
pub(crate) struct PoolReader<'a> {
    geom: crate::btree::TreeGeometry,
    read: OwnedReadBlock<'a>,
    mirrors: OwnedMirrorCount<'a>,
    read_mirror: OwnedReadMirror<'a>,
}

/// An owned block reader, the counterpart to
/// [`crate::btree::ReadBlock`]'s borrowed one.
///
/// The `Tree` borrows a reader; something has to own it, and that
/// something is [`PoolReader`].
type OwnedReadBlock<'a> = Box<dyn Fn(u64, &mut [u8]) -> Result<()> + 'a>;

/// The owned counterparts to [`crate::btree::MirrorCount`] and
/// [`crate::btree::ReadMirror`], for the same reason.
type OwnedMirrorCount<'a> = Box<dyn Fn(u64) -> Result<usize> + 'a>;
type OwnedReadMirror<'a> = Box<dyn Fn(u64, usize, &mut [u8]) -> Result<()> + 'a>;

impl PoolReader<'_> {
    /// A walker over any tree in this pool. The root address is a
    /// per-call argument, so one reader serves every tree.
    /// NOT WITNESSED. Removing the `with_redundancy` here alone leaves
    /// tests/dup_mirror_fallback.rs green: that suite damages tree roots
    /// the MOUNT reads, and this reader serves the trees a mount has
    /// already finished with. A test for it wants a damaged block inside
    /// a subvolume or csum tree, reached through a read after the mount
    /// rather than during it.
    pub(crate) fn tree(&self) -> crate::btree::Tree<'_> {
        crate::btree::Tree::new(self.geom, &*self.read)
            .with_redundancy(&*self.mirrors, &*self.read_mirror)
    }
}

#[cfg(test)]
mod root_item_target_tests {
    use super::*;
    use crate::btree::test_blocks::{geom, key, leaf, LEAF_A, NODESIZE};
    use crate::btree::Tree;

    /// A `ROOT_ITEM` body of exactly the bytes this function reads.
    ///
    /// A real one is 439 bytes; this is the shortest body from which the
    /// answer is still derivable. Two of the three copies this replaced
    /// required `len > root_item::LEVEL` (238) and would have refused
    /// it — which is the disagreement the consolidation had to settle,
    /// and settling it silently is what this test prevents.
    fn minimal_root_item(bytenr: u64) -> Vec<u8> {
        let mut body = vec![0u8; root_item::BYTENR + 8];
        body[root_item::BYTENR..root_item::BYTENR + 8].copy_from_slice(&bytenr.to_le_bytes());
        body
    }

    fn tree_over(block: Vec<u8>) -> (Vec<u8>, u64) {
        (block, LEAF_A)
    }

    fn lookup(entries: &[(DiskKey, Vec<u8>)], objectid: u64) -> Result<Option<u64>> {
        let (block, at) = tree_over(leaf(LEAF_A, crate::chunk::objectid::ROOT_TREE, entries));
        let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
            assert_eq!(logical, at, "the walk asked for a block that is not there");
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);
        root_item_target(&tree, at, objectid)
    }

    /// The version of [`lookup`] that goes through
    /// [`required_root_item_target`], for the one test that is about
    /// the "absent tree must be an error" contract rather than about
    /// `root_item_target` itself.
    fn required_lookup(entries: &[(DiskKey, Vec<u8>)], objectid: u64) -> Result<u64> {
        let (block, at) = tree_over(leaf(LEAF_A, crate::chunk::objectid::ROOT_TREE, entries));
        let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
            assert_eq!(logical, at, "the walk asked for a block that is not there");
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);
        required_root_item_target(&tree, at, objectid)
    }

    /// The bound is what the function reads, not the whole structure.
    #[test]
    fn a_root_item_long_enough_to_answer_from_is_accepted() {
        let entries = vec![(key(7, ROOT_ITEM_KEY, 0), minimal_root_item(0xABCD_0000))];
        assert_eq!(lookup(&entries, 7).unwrap(), Some(0xABCD_0000));
    }

    /// One byte short and it is refused, rather than read past.
    ///
    /// Refusing it means not matching it as a `ROOT_ITEM` for this
    /// objectid at all, so the walk completes having found nothing --
    /// the same `Ok(None)` a genuinely absent tree produces, and for the
    /// same reason: a malformed candidate is not a different flavour of
    /// error, it is simply not the item being looked for.
    #[test]
    fn a_root_item_one_byte_too_short_is_not_used() {
        let mut short = minimal_root_item(0xABCD_0000);
        short.pop();
        let entries = vec![(key(7, ROOT_ITEM_KEY, 0), short)];
        assert_eq!(
            lookup(&entries, 7).unwrap(),
            None,
            "a body that cannot hold the field must not be read from"
        );
    }

    /// Two `ROOT_ITEM`s for one objectid: the first wins.
    ///
    /// One of the three copies kept scanning and took the last. A tree
    /// with two items for the same objectid is already malformed, so
    /// neither answer is more correct — but they must not differ by
    /// which caller asked, and before this they did.
    #[test]
    fn the_first_matching_root_item_is_the_answer() {
        let entries = vec![
            (key(7, ROOT_ITEM_KEY, 0), minimal_root_item(0x1111_0000)),
            (key(7, ROOT_ITEM_KEY, 1), minimal_root_item(0x2222_0000)),
        ];
        assert_eq!(lookup(&entries, 7).unwrap(), Some(0x1111_0000));
    }

    /// A genuinely absent tree is `Ok(None)` from `root_item_target`
    /// itself -- not every caller treats that as fatal, so the error is
    /// not this function's to raise.
    #[test]
    fn a_missing_root_item_is_ok_none_not_an_error() {
        let entries = vec![(key(7, ROOT_ITEM_KEY, 0), minimal_root_item(1))];
        assert_eq!(
            lookup(&entries, 9).unwrap(),
            None,
            "root_item_target must not itself decide that absence is fatal"
        );
    }

    /// For a caller that goes through [`required_root_item_target`],
    /// that same absence becomes an error naming the tree -- this is
    /// where the `.ok()` bug of rust-fs-btrfs#114 would have hidden had
    /// it moved here instead of being fixed: a caller that needs "absent
    /// is fatal" must not be able to get that by discarding every error
    /// this reports, real failures included.
    #[test]
    fn required_root_item_target_turns_absence_into_an_error_naming_the_tree() {
        let entries = vec![(key(7, ROOT_ITEM_KEY, 0), minimal_root_item(1))];
        let err = required_lookup(&entries, 9).unwrap_err();
        assert!(
            err.to_string().contains('9'),
            "the error should name the tree that is missing, got: {err}"
        );
    }

    /// And a real failure walking the tree must not be swallowed by
    /// either form -- this is the defect itself, pinned at the source
    /// rather than only at the mount call site.
    #[test]
    fn a_real_walk_failure_is_not_confused_with_absence() {
        let read = |_logical: u64, _buf: &mut [u8]| -> Result<()> {
            Err(Error::Io("simulated device failure".to_string()))
        };
        let tree = Tree::new(geom(), &read);
        let plain = root_item_target(&tree, LEAF_A, 9);
        assert!(
            plain.is_err(),
            "a walk failure must be Err, not Ok(None) -- otherwise it is \
             indistinguishable from a tree that simply has no ROOT_ITEM"
        );
        let required = required_root_item_target(&tree, LEAF_A, 9);
        assert!(
            required.is_err(),
            "and the required form must not turn that Err into its own \
             'tree is absent' message, losing the original cause"
        );
    }

    /// An item of the right objectid but the wrong type is not a match.
    #[test]
    fn only_root_items_are_considered() {
        let entries = vec![(key(7, ROOT_ITEM_KEY + 1, 0), minimal_root_item(0x3333_0000))];
        assert_eq!(
            lookup(&entries, 7).unwrap(),
            None,
            "an item of the wrong type must not be mistaken for a match"
        );
    }

    /// The block builder's own assumption, so a change to `NODESIZE`
    /// that makes these fixtures impossible fails here rather than
    /// somewhere confusing.
    #[test]
    fn the_fixture_leaf_has_room_for_two_root_items() {
        assert!(NODESIZE as usize > 2 * (root_item::BYTENR + 8) + 128);
    }
}

#[cfg(test)]
mod sector_aligned_span_tests {
    use super::sector_aligned_span;

    /// The ordinary case: rounds the span up to whole sectors.
    #[test]
    fn a_read_inside_one_sector_spans_exactly_that_sector() {
        let sector = 4096u64;
        assert_eq!(sector_aligned_span(100, 16, sector), Some((0, 4096)));
    }

    /// A read crossing a sector boundary spans both.
    #[test]
    fn a_read_crossing_a_boundary_spans_both_sectors() {
        let sector = 4096u64;
        assert_eq!(sector_aligned_span(4090, 16, sector), Some((0, 8192)));
    }

    /// `logical + len` overflowing `u64` is refused.
    #[test]
    fn an_overflowing_add_is_refused() {
        assert_eq!(sector_aligned_span(u64::MAX - 4, 16, 4096), None);
    }

    /// THE DEFECT ITSELF: `end` is comfortably representable and the
    /// checked add above it does not fire, but rounding `end` up to the
    /// next sector overflows the multiplication. Before this fix that
    /// wrapped in release and panicked in debug; either way it must now
    /// come back `None`, not a `Some` whose `last` sits before `first`.
    #[test]
    fn a_round_up_that_overflows_the_multiplication_is_refused_not_wrapped() {
        let sector = 4096u64;
        // logical + len = u64::MAX - 10, which checked_add accepts.
        // div_ceil(4096) * 4096 on that value overflows u64.
        let logical = u64::MAX - 4096;
        let len = 4086u64;
        assert_eq!(
            sector_aligned_span(logical, len, sector),
            None,
            "the multiplication overflowed and must not silently wrap"
        );
    }

    /// A SECOND, DISTINCT WAY TO REACH THE SAME OVERFLOW, where
    /// `first` is small rather than huge.
    ///
    /// `a_round_up_that_overflows_the_multiplication_is_refused_not_wrapped`
    /// picks a `logical` near `u64::MAX`, which makes `first` huge too --
    /// and a huge `first` means that if the round-up wraps to something
    /// small, `checked_sub` catches the wrap as an underflow even
    /// without `checked_mul`'s own guard. That is not the only route to
    /// the same overflow: `logical` near zero with an enormous `len`
    /// drives `end` just as close to `u64::MAX`, while `first` stays
    /// small (here, zero) -- so a wrapped `last` can land AT OR ABOVE
    /// `first` instead of below it, and `checked_sub` sees nothing
    /// wrong. Measured directly: with the multiplication left
    /// unguarded, `logical=0, len=u64::MAX, sector=4096` wraps `last` to
    /// exactly `0`, `checked_sub(0, 0)` is `Some(0)`, and the function
    /// would return `Some((0, 0))` -- a small, entirely plausible span
    /// that is completely wrong, accepted rather than refused. This is
    /// the reason `checked_mul` is not redundant with `checked_sub`
    /// despite the other test's input suggesting it might be.
    #[test]
    fn a_small_first_does_not_let_the_multiplication_wrap_through_uncaught() {
        let sector = 4096u64;
        assert_eq!(
            sector_aligned_span(0, u64::MAX, sector),
            None,
            "logical=0 with an enormous len drives the same round-up \
             overflow while first stays small, so a wrapped `last` can \
             land at or above `first` and slip past a bare subtraction \
             undetected -- this must still be refused"
        );
    }

    /// The same overflow, exhaustively at the boundary: every `end`
    /// within the last two sectors of the address space is refused, not
    /// just one hand-picked value.
    #[test]
    fn every_end_near_u64_max_that_would_overflow_the_round_up_is_refused() {
        let sector = 4096u64;
        for len in 0..=(2 * sector) {
            let logical = u64::MAX - len;
            let result = sector_aligned_span(logical, 1, sector);
            if let Some((first, span)) = result {
                // If it did not refuse, the arithmetic must still be
                // sound: first <= logical, and the span must actually
                // cover the requested byte without wrapping.
                assert!(
                    first <= logical,
                    "first ({first:#x}) must not be after logical ({logical:#x})"
                );
                assert!(
                    first.checked_add(span).is_some(),
                    "a returned (first, span) must not itself overflow when added back \
                     together: first={first:#x} span={span:#x}"
                );
            }
        }
    }

    /// `first` never lands after `logical`, and the span always covers
    /// the requested length -- the ordinary correctness property,
    /// preserved by the extraction.
    #[test]
    fn the_span_always_covers_the_request() {
        for (logical, len, sector) in [
            (0u64, 0u64, 512u64),
            (1, 1, 512),
            (511, 2, 512),
            (512, 1, 512),
        ] {
            let Some((first, span)) = sector_aligned_span(logical, len, sector) else {
                continue;
            };
            assert!(first <= logical);
            assert!(
                first + span >= logical + len,
                "span [{first}, {}) does not cover the request [{logical}, {})",
                first + span,
                logical + len
            );
        }
    }
}

#[cfg(test)]
mod fs_and_csum_tree_roots_tests {
    use super::*;
    use crate::btree::test_blocks::{geom, key, leaf, node, LEAF_A, LEAF_B, NODESIZE, ROOT};
    use crate::btree::Tree;
    use std::collections::HashMap;

    /// A `ROOT_ITEM` body long enough for `root_item_target` to answer
    /// from.
    fn minimal_root_item(bytenr: u64) -> Vec<u8> {
        let mut body = vec![0u8; root_item::BYTENR + 8];
        body[root_item::BYTENR..root_item::BYTENR + 8].copy_from_slice(&bytenr.to_le_bytes());
        body
    }

    /// Leaf A holds the fs tree's `ROOT_ITEM` (objectid 5) as its only
    /// item; leaf B is where the csum tree's `ROOT_ITEM` (objectid 7)
    /// would live, if this fixture put one there.
    ///
    /// Keys ascend by objectid, so the walk for `FS_TREE_OBJECTID`
    /// matches on the first and only item in leaf A and stops there --
    /// `for_each`'s visitor returns `Ok(false)` on a match -- without
    /// ever reading leaf B. The walk for `CSUM_TREE_OBJECTID` finds
    /// nothing in leaf A (wrong objectid) and must continue rightward
    /// into leaf B to find out there is nothing there either -- or, in
    /// the failing variant, to hit whatever `read` does with that
    /// address.
    fn fs_tree_root_only(leaf_b: Vec<u8>) -> (HashMap<u64, Vec<u8>>, u64) {
        let a = leaf(
            LEAF_A,
            crate::chunk::objectid::ROOT_TREE,
            &[(
                key(FS_TREE_OBJECTID, ROOT_ITEM_KEY, 0),
                minimal_root_item(0xF00D),
            )],
        );
        let r = node(
            ROOT,
            crate::chunk::objectid::ROOT_TREE,
            1,
            &[
                (key(FS_TREE_OBJECTID, ROOT_ITEM_KEY, 0), LEAF_A),
                (
                    key(crate::csum::CSUM_TREE_OBJECTID, ROOT_ITEM_KEY, 0),
                    LEAF_B,
                ),
            ],
        );
        (
            HashMap::from([(LEAF_A, a), (LEAF_B, leaf_b), (ROOT, r)]),
            ROOT,
        )
    }

    /// An empty leaf B: the csum tree is genuinely absent. This is the
    /// case the original `.ok()` comment reasoned about, and it must
    /// still be `Ok((_, None))`.
    #[test]
    fn a_genuinely_absent_csum_tree_is_ok_none() {
        let empty_b = leaf(LEAF_B, crate::chunk::objectid::ROOT_TREE, &[]);
        let (blocks, root) = fs_tree_root_only(empty_b);
        let read = move |logical: u64, buf: &mut [u8]| -> Result<()> {
            let block = blocks
                .get(&logical)
                .unwrap_or_else(|| panic!("no block at {logical:#x} in this fixture"));
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);
        let (fs_root, csum_root) = fs_and_csum_tree_roots(&tree, root).unwrap();
        assert_eq!(fs_root, 0xF00D);
        assert_eq!(
            csum_root, None,
            "no ROOT_ITEM for the csum tree anywhere in this fixture"
        );
    }

    /// THE REGRESSION. Leaf B — reached only by the csum-tree search,
    /// never by the fs-tree one — fails to read. Before the fix this
    /// came back `Ok((fs_root, None))`, indistinguishable from the
    /// tree being absent, and every subsequent read for the mount ran
    /// unverified. It must now be `Err`.
    #[test]
    fn a_read_failure_reaching_only_the_csum_tree_search_is_not_confused_with_absence() {
        let mut blocks = HashMap::new();
        blocks.insert(
            LEAF_A,
            leaf(
                LEAF_A,
                crate::chunk::objectid::ROOT_TREE,
                &[(
                    key(FS_TREE_OBJECTID, ROOT_ITEM_KEY, 0),
                    minimal_root_item(0xF00D),
                )],
            ),
        );
        blocks.insert(
            ROOT,
            node(
                ROOT,
                crate::chunk::objectid::ROOT_TREE,
                1,
                &[
                    (key(FS_TREE_OBJECTID, ROOT_ITEM_KEY, 0), LEAF_A),
                    (
                        key(crate::csum::CSUM_TREE_OBJECTID, ROOT_ITEM_KEY, 0),
                        LEAF_B,
                    ),
                ],
            ),
        );
        // LEAF_B is deliberately absent from `blocks`: reading it fails.
        let read = move |logical: u64, buf: &mut [u8]| -> Result<()> {
            match blocks.get(&logical) {
                Some(block) => {
                    buf.copy_from_slice(&block[..buf.len()]);
                    Ok(())
                }
                None => Err(Error::Io(format!(
                    "simulated device failure reading {logical:#x}"
                ))),
            }
        };
        let tree = Tree::new(geom(), &read);

        let result = fs_and_csum_tree_roots(&tree, ROOT);
        assert!(
            result.is_err(),
            "a read failure reaching leaf B while searching for the csum \
             tree must propagate, not collapse into Ok((fs_root, None)) \
             the way rust-fs-btrfs#114 describes"
        );
    }

    /// The fixture's own precondition: the fs-tree lookup really does
    /// stop before touching leaf B, so the test above is exercising the
    /// csum-tree search specifically and not incidentally failing on
    /// the fs-tree one.
    #[test]
    fn the_fs_tree_lookup_alone_never_touches_leaf_b() {
        let a = leaf(
            LEAF_A,
            crate::chunk::objectid::ROOT_TREE,
            &[(
                key(FS_TREE_OBJECTID, ROOT_ITEM_KEY, 0),
                minimal_root_item(0xF00D),
            )],
        );
        let r = node(
            ROOT,
            crate::chunk::objectid::ROOT_TREE,
            1,
            &[
                (key(FS_TREE_OBJECTID, ROOT_ITEM_KEY, 0), LEAF_A),
                (
                    key(crate::csum::CSUM_TREE_OBJECTID, ROOT_ITEM_KEY, 0),
                    LEAF_B,
                ),
            ],
        );
        let blocks = HashMap::from([(LEAF_A, a), (ROOT, r)]);
        let read = move |logical: u64, buf: &mut [u8]| -> Result<()> {
            let block = blocks.get(&logical).unwrap_or_else(|| {
                panic!("the fs-tree lookup touched {logical:#x}, expected only ROOT and LEAF_A")
            });
            buf.copy_from_slice(&block[..buf.len()]);
            Ok(())
        };
        let tree = Tree::new(geom(), &read);
        let found = required_root_item_target(&tree, ROOT, FS_TREE_OBJECTID).unwrap();
        assert_eq!(found, 0xF00D);
    }

    /// The block builder's own assumption.
    #[test]
    fn the_fixture_leaf_has_room_for_the_root_item() {
        assert!(NODESIZE as usize > 2 * (root_item::BYTENR + 8) + 128);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{block_group, Chunk, Stripe};

    /// A device that answers every read with one byte value, so which
    /// device a read landed on is visible in the bytes it returned.
    struct Marked(u8, u64);

    impl BlockRead for Marked {
        fn read_at(&self, _offset: u64, buf: &mut [u8]) -> fs_core::Result<()> {
            buf.fill(self.0);
            Ok(())
        }
        fn size_bytes(&self) -> u64 {
            self.1
        }
    }

    /// A two-device RAID1 chunk whose FIRST copy is on devid 2 — the
    /// arrangement the shipped pool fixture does not have.
    fn raid1_stripe0_on_devid2() -> ChunkMap {
        let mut map = ChunkMap::new();
        map.insert(Chunk {
            logical: 0x100000,
            length: 0x100000,
            owner: 2,
            stripe_len: 0x10000,
            chunk_type: block_group::METADATA | block_group::RAID1,
            io_align: 4096,
            io_width: 4096,
            sector_size: 4096,
            num_stripes: 2,
            sub_stripes: 0,
            stripes: vec![
                Stripe {
                    devid: 2,
                    offset: 0x200000,
                    dev_uuid: [0; 16],
                },
                Stripe {
                    devid: 1,
                    offset: 0x300000,
                    dev_uuid: [0; 16],
                },
            ],
        })
        .expect("a two-stripe RAID1 chunk is well-formed");
        map
    }

    /// THE SEMANTIC THE MOUNT PATH'S PRIMARY READ DEPENDS ON, WITNESSED
    /// WITHOUT A FIXTURE.
    ///
    /// `read_logical` reads whichever handle it was given at the
    /// physical offset the mapping names, IGNORING the devid the
    /// mapping names. On a pool that is a read of the right offset on
    /// the wrong disk, and it returns wrong bytes rather than failing —
    /// the one outcome a caller cannot detect. `read_logical_pool`
    /// resolves the devid.
    ///
    /// # WHY THIS IS A UNIT TEST AND NOT A FIXTURE TEST
    ///
    /// `test-disks/btrfs-pool-{a,b}.img` cannot show the difference.
    /// Measured on it: every chunk — system, metadata and data — puts
    /// stripe 0 on devid 1, which is also the handle `mount_pool` picks
    /// (`by_id.values().next()`, so the lowest devid). So on that
    /// fixture the two functions agree for every address, and reverting
    /// any of the three call sites to `read_logical` leaves
    /// `tests/dup_mirror_fallback.rs` green — measured, three arms, all
    /// `EXIT=0`, 7 passed.
    ///
    /// That is why this test exists at this level: it holds the
    /// difference the call sites rely on.
    ///
    /// # WHAT WITNESSES THE CALL SITES IS THE COMPILER
    ///
    /// No test here can, on the shipped fixture. So the two wrappers
    /// that made the wrong choice available -- `read_logical` and
    /// `read_logical_on` -- are deleted rather than left unused, and
    /// reverting any of the three mount-path sites to one of them is a
    /// COMPILE ERROR rather than a green suite. That is the control,
    /// and it is a stronger one than a failing assertion.
    ///
    /// Still open, and not this job: a second pool fixture whose stripe
    /// 0 is on the higher devid -- `mkfs.btrfs` produces one if the
    /// devices are given in the other order -- would let
    /// `tests/dup_mirror_fallback.rs` witness the routing end to end
    /// through `mount_pool`. That is a change to the `pool` recipe in
    /// `test-disks/guest-build-images.sh`.
    #[test]
    fn the_primary_read_of_a_pool_follows_the_devid_not_only_the_offset() {
        let map = raid1_stripe0_on_devid2();
        let logical = 0x100000;

        // Stripe 0 really is on devid 2, or this test is measuring the
        // arrangement it was written to avoid.
        let m0 = map.map_mirror(logical, 0).expect("mirror 0");
        let m1 = map.map_mirror(logical, 1).expect("mirror 1");
        assert_eq!(m0.devid, 2, "stripe 0 must be on devid 2 for this test");
        assert_eq!(m1.devid, 1, "stripe 1 must be on devid 1 for this test");

        let dev1: Arc<dyn BlockRead> = Arc::new(Marked(0x11, 1 << 30));
        let dev2: Arc<dyn BlockRead> = Arc::new(Marked(0x22, 1 << 30));
        let devices: BTreeMap<u64, Arc<dyn BlockRead>> =
            [(1, dev1.clone()), (2, dev2.clone())].into_iter().collect();

        // `mount_pool` hands the lowest devid to `device`, so that is
        // what the single-device form would read.
        let device = dev1.clone();

        let mut buf = [0u8; 8];
        Filesystem::read_logical_pool(&device, &devices, &map, logical, &mut buf)
            .expect("the pool read must succeed");
        assert_eq!(
            buf, [0x22; 8],
            "copy 0 lives on devid 2, so the pool read must return devid 2's bytes"
        );

        // What the deleted `read_logical` did, spelled against the
        // primitive that survives: mirror 0, devid unchecked. Written
        // this way so the contrast keeps its meaning now that the
        // wrapper it used to name is gone.
        let mut buf = [0u8; 8];
        Filesystem::read_logical_on_mirror(&device, &map, u64::MAX, 0, logical, &mut buf)
            .expect("the devid-unchecked read succeeds -- that is the problem");
        assert_eq!(
            buf, [0x11; 8],
            "the single-device form returned devid 1's bytes for a copy that is on \
             devid 2, and reported success: wrong data rather than an error, which is \
             why the mount path must not use it on a pool"
        );
    }
}
