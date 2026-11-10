//! Writing an ordinary, copy-on-write file (#61).
//!
//! [`crate::write`] overwrites a `nodatacow` file where its bytes lie.
//! Every other file is copy-on-write: an extent, once committed, is never
//! changed, so writing a byte of one means a new extent holding the new
//! bytes, the file's item pointed at it, and the old one released — then
//! all of that committed as one transaction, so a crash leaves either the
//! old file or the new one and never a mixture.
//!
//! # What a write does
//!
//! For each extent item the range touches:
//!
//! 1. **A new extent** is found in a data block group
//!    (`Filesystem::find_data_extents`), as long as the bytes the item
//!    covers, and filled with them — the old contents with the write
//!    applied. The copy is the item's window of the old extent, not the
//!    whole of it, so the new item references all of its extent from
//!    offset zero.
//! 2. **The file's `EXTENT_DATA` item** is pointed at it, in place: the
//!    item keeps its size, so its leaf cannot overflow.
//! 3. **The extent tree** loses the old extent's `EXTENT_ITEM` and gains
//!    one for the new, with an inline `EXTENT_DATA_REF` back to the file.
//! 4. **The free-space tree** and the **block groups' `used`** follow, and
//!    the superblock's `bytes_used` moves by whatever the copy is shorter
//!    than the extent it replaced.
//! 5. **The inode** is stamped with the transaction, a new change count
//!    and new change and modification times.
//!
//! The data goes to every mirror first; then the tree blocks, a flush,
//! the superblocks and a flush, in [`Filesystem::commit`]'s order. Until
//! the superblocks land, nothing points at the new extent.
//!
//! # What it refuses
//!
//! Each refusal is a case not yet written, named so a caller can tell
//! which it met:
//!
//! - a **checksummed** file: its new extent needs `EXTENT_CSUM` items and
//!   its old one's removed, and the checksum tree is not written yet;
//! - a write that **grows** the file, or lands in a **hole**: both
//!   allocate where the file has no extent item to repoint;
//! - an **inline**, **preallocated** or **compressed** extent: each
//!   changes the item's kind, not only where it points;
//! - a **shared** extent — more than one reference, or one the last
//!   snapshot could still be reading: releasing it would free bytes
//!   another reader holds.
//!
//! A write in a `nodatacow` file goes to [`Filesystem::write_at`], which
//! writes in place and commits nothing.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::chunk::{objectid, DiskKey};
use crate::error::{Error, Result};
use crate::fs::{Filesystem, EXTENT_DATA_KEY};
use crate::inode::{Inode, INODE_ITEM_KEY, INODE_NODATACOW, INODE_NODATASUM};
use crate::super_write::Commit;
use crate::transaction::{DataMove, DataWrite};
use crate::write::window_inside_extent;

/// How many rounds a write's plan has to close over its own bookkeeping.
/// Each round adds the extent and free-space leaves the last one's moves
/// dirtied; a write that has not settled in this many is refused.
const PLAN_ROUNDS: usize = 64;

/// One extent item the write replaces, as the planner found it.
#[derive(Debug, Clone, Copy)]
struct Target {
    /// The item's key offset: where in the file it begins.
    start: u64,
    /// How much of the file it covers, its `num_bytes`.
    len: u64,
    /// The extent it references.
    extent_start: u64,
    /// That extent's length, from the extent tree.
    extent_len: u64,
    /// Where in that extent the item's window begins.
    extent_offset: u64,
}

impl Filesystem {
    /// Write `data` at `offset` in a regular file, and commit it.
    ///
    /// A `nodatacow` file is written in place, exactly as
    /// [`Filesystem::write_at`] does. Any other file is written
    /// copy-on-write, as one committed transaction per call — see the
    /// [module documentation](crate::cow_write) for what that changes and what it
    /// still refuses. Nothing is written unless the whole range can be.
    ///
    /// Takes `&mut self` because a commit moves the trees this mount
    /// reads: once it lands, the mount is reopened on the new
    /// generation, so the next read or write starts from what was just
    /// committed rather than from the superseded roots.
    ///
    /// # Errors
    ///
    /// [`Error::ReadOnly`] unless mounted with [`Filesystem::mount_rw`],
    /// [`Error::NotAFile`] for anything but a regular file, and
    /// [`Error::UnsupportedFeature`] naming the case for every refusal.
    /// If reopening the mount after the commit fails, the write is
    /// committed and the error is returned; this handle then still reads
    /// the previous generation and must be dropped.
    pub fn write(&mut self, ino: u64, offset: u64, data: &[u8]) -> Result<usize> {
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
        if inode.flags & INODE_NODATACOW != 0 {
            return self.write_at(ino, offset, data);
        }
        self.write_cow(&inode, offset, data)
    }

    fn write_cow(&mut self, inode: &Inode, offset: u64, data: &[u8]) -> Result<usize> {
        let ino = inode.ino;
        if inode.flags & INODE_NODATASUM == 0 {
            return Err(Error::UnsupportedFeature(format!(
                "inode {ino} is copy-on-write and checksummed, and writing it means writing \
                 checksum items for its new extents, which is not implemented"
            )));
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| Error::UnsupportedFeature("write range overflows".into()))?;
        if end > inode.size {
            return Err(Error::UnsupportedFeature(format!(
                "inode {ino}: writing to {end} would grow the file past its {} bytes, which \
                 adds an extent rather than replacing one",
                inode.size
            )));
        }

        // Every refusal before anything is allocated.
        let targets = self.plan_cow_targets(ino, offset, end)?;
        self.refuse_checksummed_extents(ino, &targets)?;

        let lens: Vec<u64> = targets.iter().map(|t| t.len).collect();
        let news = self.find_data_extents(&lens)?;

        // Each copy: the item's bytes as they read now, with the write
        // laid over them. Past the end of the file the last sector reads
        // as zeros, and that is what the copy holds there.
        let mut contents: Vec<(u64, Vec<u8>)> = Vec::with_capacity(targets.len());
        for (t, &at) in targets.iter().zip(&news) {
            let size = usize::try_from(t.len)
                .map_err(|_| Error::UnsupportedFeature(format!("an extent of {} bytes", t.len)))?;
            let mut buf = vec![0u8; size];
            self.read_at(ino, t.start, &mut buf)?;
            let from = offset.max(t.start);
            let to = end.min(t.start + t.len);
            buf[(from - t.start) as usize..(to - t.start) as usize]
                .copy_from_slice(&data[(from - offset) as usize..(to - offset) as usize]);
            contents.push((at, buf));
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let write = DataWrite {
            root: objectid::FS_TREE,
            ino,
            moves: targets
                .iter()
                .zip(&news)
                .map(|(t, &new)| DataMove {
                    old: t.extent_start,
                    old_len: t.extent_len,
                    old_ref_offset: t.start - t.extent_offset,
                    new,
                    len: t.len,
                    file_offset: t.start,
                })
                .collect(),
            time: (now.as_secs(), now.subsec_nanos()),
        };

        // The fs tree leaves the write edits: the inode item's, and each
        // extent item's. Everything else the transaction touches follows
        // from these and from the data moves.
        let dirty: Vec<u64> = {
            let reader = self.pool_reader();
            let tree = reader.tree();
            let mut leaves = BTreeSet::new();
            let keys = std::iter::once((INODE_ITEM_KEY, 0))
                .chain(targets.iter().map(|t| (EXTENT_DATA_KEY, t.start)));
            for (key_type, key_offset) in keys {
                let key = DiskKey {
                    objectid: ino,
                    key_type,
                    offset: key_offset,
                };
                leaves.insert(tree.descend(self.fs_tree_root, &key)?.header.bytenr);
            }
            leaves.into_iter().collect()
        };

        let generation = self.sb.generation.checked_add(1).ok_or_else(|| {
            Error::UnsupportedFeature("the generation counter is exhausted".into())
        })?;
        let plan = self.plan_transaction_closed_with(&dirty, &write, PLAN_ROUNDS)?;
        let blocks = self.render_plan_with(&plan, &write, generation)?;
        let root = self.planned_root(&plan).ok_or_else(|| {
            Error::UnsupportedFeature("the write's plan does not move the root tree".into())
        })?;

        // `bytes_used` is the sum of every group's `used`, which the plan
        // moved by each copy's length less its extent's.
        let delta = plan.usage_delta(u64::from(self.sb.nodesize))
            + write
                .moves
                .iter()
                .map(|m| i128::from(m.len) - i128::from(m.old_len))
                .sum::<i128>();
        let bytes_used = match delta {
            0 => None,
            d => Some(
                u64::try_from(i128::from(self.sb.bytes_used) + d).map_err(|_| {
                    Error::UnsupportedFeature(format!(
                        "the superblock's {} bytes used cannot move by {d}",
                        self.sb.bytes_used
                    ))
                })?,
            ),
        };

        // The data, to every mirror, before any tree block: nothing
        // names these addresses until the superblocks are written, and
        // the commit's first flush puts them on the device before that.
        let device = Arc::clone(self.writable.as_ref().expect("checked by write"));
        for (at, buf) in &contents {
            Self::mirror_spans(&device, &self.map, *at, buf.len())?;
        }
        for (at, buf) in &contents {
            Self::write_logical_all_mirrors(&device, &self.map, *at, buf)?;
        }
        self.commit(
            &blocks,
            &Commit {
                generation,
                root,
                bytes_used,
                // Maintained, by `apply_free_space`, so it stays valid.
                invalidate_free_space_tree: false,
                ..Default::default()
            },
        )?;

        // The trees this mount holds are the previous generation's.
        *self = Filesystem::mount_rw(device)?;
        Ok(data.len())
    }

    /// The extent items `[offset, end)` touches, each checked writable.
    ///
    /// Every refusal happens here, while nothing is allocated. The checks
    /// are [`Filesystem::write_at`]'s, for the same reasons: a window
    /// outside its extent, more than one reference, or an extent from at
    /// or before the last snapshot.
    fn plan_cow_targets(&self, ino: u64, offset: u64, end: u64) -> Result<Vec<Target>> {
        let pieces = self.file_extents(ino)?;
        let last_snapshot = self.fs_tree_last_snapshot()?;
        let mut out = Vec::new();
        let mut pos = offset;
        while pos < end {
            let Some(piece) = pieces
                .iter()
                .find(|p| pos >= p.start && pos < p.start + p.len)
            else {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} is a hole, and filling it adds an extent \
                     rather than replacing one"
                )));
            };
            if piece.compressed {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} is in a compressed extent, and replacing one \
                     is not implemented"
                )));
            }
            let Some(logical) = piece.logical else {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} is inline or preallocated, and writing it \
                     changes the item's kind, which is not implemented"
                )));
            };
            let (refs, extent_len, generation) = self.extent_item(piece.extent_start)?;
            if !window_inside_extent(logical, piece.len, piece.extent_start, extent_len) {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: offset {pos} maps to [{logical}, +{}), outside the \
                     {extent_len}-byte extent the extent tree records at {}",
                    piece.len, piece.extent_start
                )));
            }
            if refs != 1 {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: the extent at {} has {refs} references, so a snapshot or \
                     another file still reads it, and releasing one reference of several \
                     is not implemented",
                    piece.extent_start
                )));
            }
            if generation <= last_snapshot {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino}: the extent at {} is from generation {generation}, at or \
                     before the snapshot taken at {last_snapshot}, so the snapshot may still \
                     be reading it",
                    piece.extent_start
                )));
            }
            out.push(Target {
                start: piece.start,
                len: piece.len,
                extent_start: piece.extent_start,
                extent_len,
                extent_offset: logical - piece.extent_start,
            });
            pos = piece.start + piece.len;
        }
        Ok(out)
    }

    /// Refuse an extent the checksum tree has items for.
    ///
    /// A `nodatasum` file's extents carry none, and releasing one leaves
    /// nothing behind in that tree. One that does have them would leave
    /// checksums for a range nothing allocates — which `btrfs check`
    /// reports — so it is refused rather than released.
    fn refuse_checksummed_extents(&self, ino: u64, targets: &[Target]) -> Result<()> {
        let Ok(root) = self.tree_root(crate::csum::CSUM_TREE_OBJECTID) else {
            return Ok(());
        };
        let reader = self.pool_reader();
        let tree = reader.tree();
        for t in targets {
            let found = crate::csum::digests_for_range(
                &tree,
                root,
                self.sb.csum_type.digest_len(),
                u64::from(self.sb.sectorsize),
                t.extent_start,
                t.extent_len,
            )?;
            if !found.is_empty() {
                return Err(Error::UnsupportedFeature(format!(
                    "inode {ino} is nodatasum, yet the checksum tree holds {} checksums for its \
                     extent at {}; releasing the extent would leave them describing nothing",
                    found.len(),
                    t.extent_start
                )));
            }
        }
        Ok(())
    }
}
