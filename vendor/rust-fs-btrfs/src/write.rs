//! Overwriting file data in place.
//!
//! Btrfs is copy-on-write, so as a rule nothing is written where it
//! already is: a change allocates a new block, records a new checksum,
//! rewrites the B-tree path to the root and commits a new superblock
//! generation. None of that is possible without a transaction engine,
//! and a partial attempt at one produces a filesystem its own checker
//! rejects.
//!
//! There is one exception, and it is the whole of what this module does.
//!
//! # `nodatacow`
//!
//! A file marked `NODATACOW` is deliberately exempt from copy-on-write:
//! its data blocks are overwritten where they lie. Btrfs also clears
//! checksumming for such a file — the flag is set together with
//! `NODATASUM`, because a block that changes in place cannot keep a
//! checksum that was computed elsewhere and committed separately.
//!
//! So for these files, and only these, writing the bytes changes nothing
//! else. No block is allocated. No checksum item exists to update. No
//! tree node is rewritten and no generation is committed. The write is
//! exactly as safe as the equivalent on a filesystem that never had
//! copy-on-write to begin with.
//!
//! # The one thing that is not obvious
//!
//! `NODATACOW` is not a promise that the extent will be written in
//! place. It is a promise that it will be written in place **while it
//! belongs to one file**. Take a snapshot and the extent becomes shared;
//! the next write to it copies first, because the snapshot must keep
//! seeing what it saw. A driver that honoured the flag and skipped that
//! check would silently rewrite what a snapshot is still pointing at.
//!
//! Sharing is visible in the extent tree, as a reference count on the
//! extent item, but not only there. A snapshot of a tree taller than one
//! leaf adds references to the tree blocks under the root, not to the
//! data extents, so the count stays at one while the snapshot reads the
//! extent (#173). The kernel's nocow check handles that case with the
//! tree's `last_snapshot`: an extent from that generation or earlier may be
//! shared. This refuses a count above one, and also any extent the last
//! snapshot could still be reading. Those lookups are the reason this
//! module is more than a byte copy.

use crate::chunk::{key_type, objectid, DiskKey};
use crate::error::{Error, Result};
use crate::fs::Filesystem;

/// The inode flags this module reads, defined beside `Inode::flags` and
/// re-exported here where callers already name them.
pub use crate::inode::{INODE_NODATACOW, INODE_NODATASUM};

/// Offsets within `btrfs_extent_item`.
mod extent_item {
    /// How many references point at this extent. One means it belongs
    /// to a single file and nothing else is looking at it.
    pub const REFS: usize = 0;
    /// The transaction that allocated the extent.
    pub const GENERATION: usize = 8;
}

impl Filesystem {
    /// Overwrite `data` at `offset` in a `nodatacow` file.
    ///
    /// Returns the number of bytes written, always `data.len()` on
    /// success. Nothing is written unless the whole range can be, so a
    /// range that turns out to be unwritable partway through leaves the
    /// file untouched rather than half updated.
    ///
    /// # Errors
    ///
    /// [`Error::ReadOnly`] unless opened with [`Filesystem::mount_rw`],
    /// and [`Error::UnsupportedFeature`] naming which condition failed
    /// for everything else — a caller deciding whether to fall back
    /// needs to know whether it met a shared extent or a compressed one,
    /// not merely that the write was declined.
    pub fn write_at(&self, ino: u64, offset: u64, data: &[u8]) -> Result<usize> {
        if self.writable.is_none() {
            return Err(Error::ReadOnly);
        }
        if data.is_empty() {
            return Ok(0);
        }

        let inode = self.read_inode(ino)?;
        if !inode.is_regular_file() {
            return Err(Error::NotAFile);
        }
        if inode.flags & INODE_NODATACOW == 0 {
            return Err(Error::UnsupportedFeature(format!(
                "inode {ino} is copy-on-write, so overwriting it in place would leave the \
                 extent tree, the checksum tree and the superblock generation describing \
                 something that is no longer there"
            )));
        }
        if inode.flags & INODE_NODATASUM == 0 {
            return Err(Error::UnsupportedFeature(format!(
                "inode {ino} is nodatacow but still checksummed, so writing it in place \
                 would leave every checksum item for it wrong"
            )));
        }

        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| Error::UnsupportedFeature("write range overflows".into()))?;
        if end > inode.size {
            return Err(Error::UnsupportedFeature(format!(
                "inode {ino}: writing to {end} would grow the file past its {} bytes, \
                 which allocates",
                inode.size
            )));
        }

        // Resolve everything before writing anything.
        let plan = self.plan_nodatacow_write(ino, offset, data.len())?;

        // EVERY MIRROR, AND EVERY SPAN CHECKED FIRST (#71). This wrote
        // copy 0 only, so a `-d dup` or `raid1` file kept its old bytes in
        // the other copy with no checksum to arbitrate -- the file is
        // nodatasum by the check above -- and never checked a stripe
        // against the device. Resolving every mirror of every piece
        // before the first write keeps "nothing is written unless the
        // whole range can be".
        let device = self.writable.as_ref().expect("checked above");
        let mut done = 0usize;
        for &(logical, len) in &plan {
            Self::mirror_spans(device, &self.map, logical, len)?;
            done += len;
        }
        debug_assert_eq!(done, data.len());
        done = 0;
        for (logical, len) in plan {
            Self::write_logical_all_mirrors(device, &self.map, logical, &data[done..done + len])?;
            done += len;
        }
        device.flush()?;
        Ok(done)
    }

    /// Whether `ino` can be written in place at all.
    ///
    /// Answers the question a caller actually has before offering a file
    /// as editable, rather than making them attempt a write and read the
    /// refusal. It applies the same conditions the write does, to the
    /// whole file rather than to one range — a file is reported writable
    /// only if every extent of it could be overwritten.
    ///
    /// A file with no extents at all — empty, or entirely holes — is
    /// reported writable, since there is nothing there that would have
    /// to be refused. Any write to it would still be refused for
    /// exceeding its size, which is the correct answer for a different
    /// reason.
    pub fn can_write_in_place(&self, ino: u64) -> Result<bool> {
        let inode = self.read_inode(ino)?;
        if !inode.is_regular_file() {
            return Ok(false);
        }
        if inode.flags & (INODE_NODATACOW | INODE_NODATASUM) != (INODE_NODATACOW | INODE_NODATASUM)
        {
            return Ok(false);
        }
        let last_snapshot = self.fs_tree_last_snapshot()?;
        // A FILE WHOSE EXTENTS DO NOT DECODE IS NOT WRITABLE, and that is
        // the answer to give rather than the error. Reading them refuses a
        // window that leaves its extent (#188), which is one of the very
        // conditions this loop is here to report — so a caller asking "can
        // I edit this?" gets "no", and learns why from the write itself.
        let pieces = match self.file_extents(ino) {
            Ok(pieces) => pieces,
            Err(Error::BadSuperblock(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        for piece in pieces {
            let Some(logical) = piece.logical else {
                return Ok(false);
            };
            if piece.compressed {
                return Ok(false);
            }
            // Both of the write's extent-tree checks, not only the
            // reference count (Greptile on #156): a window outside the
            // extent's recorded length is refused by every write, so a
            // file holding one is not writable.
            let (refs, extent_len, generation) = self.extent_item(piece.extent_start)?;
            if refs != 1
                || generation <= last_snapshot
                || !window_inside_extent(logical, piece.len, piece.extent_start, extent_len)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Where each part of the write lands, as (logical address, length).
    ///
    /// Every refusal happens here, while the file is still untouched.
    fn plan_nodatacow_write(&self, ino: u64, offset: u64, len: usize) -> Result<Vec<(u64, usize)>> {
        let pieces = self.file_extents(ino)?;
        let last_snapshot = self.fs_tree_last_snapshot()?;
        let mut plan = Vec::new();
        let mut done = 0usize;

        while done < len {
            let pos = offset + done as u64;
            let Some(piece) = pieces
                .iter()
                .find(|p| pos >= p.start && pos < p.start + p.len)
            else {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} is a hole, and filling it would allocate"
                )));
            };
            if piece.compressed {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} is in a compressed extent, which cannot be \
                     rewritten without recompressing the whole of it"
                )));
            }
            let Some(logical) = piece.logical else {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} is inline or preallocated, so writing it \
                     changes the item rather than the blocks it points at"
                )));
            };

            // The check that `nodatacow` alone does not give us.
            let (refs, extent_len, generation) = self.extent_item(piece.extent_start)?;
            // INSIDE THE EXTENT, BY THE EXTENT TREE'S RECORD OF IT (#89).
            // The window's bound in `decode_extent` is `ram_bytes`, a field
            // of the same item that moved the window, so a crafted item
            // raised both and sent this write past the extent -- over a
            // tree block, or another file -- while the reference check
            // below, keyed on the extent's start, still found one owner.
            // The EXTENT_ITEM's key offset is the length the allocator
            // recorded, and nothing in the file's item can change it.
            if !window_inside_extent(logical, piece.len, piece.extent_start, extent_len) {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} maps to [{logical}, +{}), outside the \
                     {extent_len}-byte extent the extent tree records at {}",
                    piece.len, piece.extent_start
                )));
            }
            if refs != 1 {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: the extent at {} has {refs} references, so something \
                     else — most likely a snapshot — is still reading it",
                    piece.extent_start
                )));
            }
            // ONE REFERENCE IS NOT ONE READER (#173). A snapshot of a tree
            // whose root is a node raises the counts of the tree blocks
            // under it, not of the data extents those blocks lead to, so
            // an extent the snapshot reads can still count one. Anything
            // the snapshot could see was allocated at or before it.
            if generation <= last_snapshot {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: the extent at {} is from generation {generation}, at or \
                     before the snapshot taken at {last_snapshot}, so the snapshot may \
                     still be reading it",
                    piece.extent_start
                )));
            }

            let within = pos - piece.start;
            let chunk = ((piece.len - within) as usize).min(len - done);
            plan.push((logical + within, chunk));
            done += chunk;
        }
        Ok(plan)
    }

    /// How many references the extent beginning at `bytenr` has, how
    /// long the extent tree records it as, and the generation that
    /// allocated it.
    ///
    /// One reference means it belongs to a single file. Anything more
    /// means a snapshot or a reflink is also pointing at it, and writing
    /// in place would change what that other reader sees. The length is
    /// the `EXTENT_ITEM`'s key offset: what the allocator reserved.
    pub(crate) fn extent_item(&self, bytenr: u64) -> Result<(u64, u64, u64)> {
        let root = self.extent_tree_root()?;
        let reader = self.pool_reader();
        let tree = reader.tree();

        // SOUGHT, NOT WALKED. This read every item in the extent tree until
        // it met the one it wanted, which is the whole tree for an extent
        // near the end. A write does it once and could afford that; a read
        // does it per extent (#188), and a file of a thousand extents would
        // have walked the tree a thousand times.
        let mut refs = None;
        for item in tree.find_all(root, bytenr, key_type::EXTENT_ITEM)? {
            if item.data.len() >= extent_item::GENERATION + 8 {
                let data = &item.data;
                let le64 =
                    |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().expect("8 bytes"));
                refs = Some((
                    le64(extent_item::REFS),
                    item.key.offset,
                    le64(extent_item::GENERATION),
                ));
                break;
            }
        }

        // An extent with no item is not "unreferenced" — it is an extent
        // this driver failed to find, and treating the two the same
        // would turn a lookup bug into a write over shared data.
        refs.ok_or_else(|| {
            Error::UnsupportedFeature(format!(
                "the extent tree holds no item for the extent at {bytenr}, so whether it \
                 is shared cannot be established"
            ))
        })
    }

    /// The generation the default subvolume was last snapshotted at, or
    /// zero if it never was.
    ///
    /// A writable mount writes only the default subvolume (a subvolume
    /// handle never carries the write capability), so this is the tree
    /// every write edits.
    pub(crate) fn fs_tree_last_snapshot(&self) -> Result<u64> {
        use crate::fs::{root_item, FS_TREE_OBJECTID, ROOT_ITEM_KEY};
        let reader = self.pool_reader();
        let mut found = None;
        reader
            .tree()
            .for_each(self.sb.root, &mut |key: &DiskKey, data: &[u8]| {
                if key.objectid == FS_TREE_OBJECTID && key.key_type == ROOT_ITEM_KEY {
                    found = data
                        .get(root_item::LAST_SNAPSHOT..root_item::LAST_SNAPSHOT + 8)
                        .map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")));
                    return Ok(false);
                }
                Ok(true)
            })?;
        // No root item, or one too short to hold the field, is not "never
        // snapshotted": treating the two the same would turn a misread
        // into a write over shared data.
        found.ok_or_else(|| {
            Error::UnsupportedFeature(
                "the root tree holds no readable ROOT_ITEM for the default subvolume, so \
                 whether a snapshot shares its extents cannot be established"
                    .into(),
            )
        })
    }

    /// The extent tree's root, named by the root tree.
    ///
    /// A thin alias now: this used to be `tree_root(EXTENT_TREE)`
    /// written out again with its own bound and its own objectid
    /// constant, and its bound disagreed with the general one. See
    /// [`crate::fs::root_item_target`].
    fn extent_tree_root(&self) -> Result<u64> {
        self.tree_root(objectid::EXTENT_TREE)
    }
}

/// Whether a file piece's window, `[logical, logical + len)`, lies inside
/// the extent the extent tree records at `extent_start` for `extent_len`
/// bytes. Shared by the write and by `can_write_in_place`, so the two
/// cannot disagree about which windows a write refuses.
pub(crate) fn window_inside_extent(
    logical: u64,
    len: u64,
    extent_start: u64,
    extent_len: u64,
) -> bool {
    match (
        logical.checked_add(len),
        extent_start.checked_add(extent_len),
    ) {
        (Some(piece_end), Some(extent_end)) => logical >= extent_start && piece_end <= extent_end,
        _ => false,
    }
}
