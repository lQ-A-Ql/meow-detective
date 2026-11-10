//! Performing a commit: the writes, in the order the kernel makes them.
//!
//! Every other piece of the write path produces bytes. This is the one
//! that puts them on the device, and the ORDER it does that in is the
//! crash-consistency — not a performance detail. A commit written in the
//! wrong order is not slower, it is a filesystem that a power cut turns
//! into one referencing tree blocks that were never written.
//!
//! # The order, observed
//!
//! Not reasoned. `scripts/trace-commit.sh` records a live commit with
//! `blktrace` and keeps the `D` events — the moment each request reached
//! the device. One `touch` and one `sync`, identical across three runs:
//!
//! ```text
//! D WSM  76160  32     ┐
//! D WSM 141696  32     │ four tree blocks, as two DUP pairs
//! D WSM 141728 128     │
//! D WSM  76192 128     ┘
//! D FN                 <- flush
//! D WSM    128   8     superblock at 64 KiB
//! D WSM 131072   8     superblock at 64 MiB
//! D FN                 <- flush
//! ```
//!
//! Four things that follow, two of which nobody had reasoned:
//!
//! **Both mirrors of a DUP block go out before the barrier.** A torn
//! write to one leaves the other, and the barrier still orders both
//! against the superblock.
//!
//! **There is a second flush, after the superblocks.**
//!
//! **The superblocks carry no FUA.** The flags are `WSM` —
//! write/sync/metadata — not the `A` that force-unit-access would show.
//! Durability of the commit point comes from the trailing flush, not
//! from the write. A writer that set FUA and dropped the second flush
//! would be doing something the kernel does not; one that dropped both
//! would have no commit point at all.
//!
//! **The superblock copies are written in address order**, and every
//! copy the device is large enough to hold is written.
//!
//! # What this does not decide
//!
//! It does not choose what to write. Deciding which blocks a change
//! produces, allocating them, and recording those allocations in the
//! extent tree all happen first — and the last of those is recursive,
//! because recording an allocation modifies a tree that itself lives in
//! allocated blocks. That is the piece this is waiting on, not a
//! shortcut taken here.
//!
//! So this takes blocks that are already built and already placed. It is
//! the last step of a transaction and only the last step.

use crate::chunk::objectid;
use crate::error::{Error, Result};
use crate::fs::root_item;
use crate::fs::Filesystem;
use crate::super_write::SUPERBLOCK_SIZE;
use crate::super_write::{self, Commit};
use crate::superblock::SUPER_OFFSETS;

/// One tree block, built and placed, waiting to be written.
#[derive(Debug, Clone)]
pub struct PlacedBlock {
    /// Where it goes, in logical address space.
    pub logical: u64,
    /// Its bytes, checksum already stamped.
    pub bytes: Vec<u8>,
}

impl Filesystem {
    /// Write a transaction and make it real.
    ///
    /// `blocks` are written first, to every mirror, then a flush, then
    /// the superblock to every copy, then a second flush. Until the
    /// superblocks land, none of it counts: a reader still sees the
    /// previous root.
    ///
    /// # Errors
    ///
    /// [`Error::ReadOnly`] unless opened with [`Filesystem::mount_rw`].
    ///
    /// A failure BEFORE the superblocks leaves the filesystem exactly as
    /// it was — the blocks written are ones nothing points at, which is
    /// what copy-on-write makes safe and is precisely why the order is
    /// this way round.
    ///
    /// A failure DURING the superblock writes is the one case that is
    /// not clean, and it cannot be made clean by anything here: some
    /// copies name the new root and some the old. That is the same state
    /// a power cut produces, it is what the generation number in each
    /// copy exists to resolve, and a reader picks the newest copy that
    /// verifies.
    pub fn commit(&self, blocks: &[PlacedBlock], commit: &Commit) -> Result<()> {
        let Some(device) = self.writable.as_ref() else {
            return Err(Error::ReadOnly);
        };

        // 0. The superblock image, backup slot included, before anything is
        //    written: its roots are read out of the new root tree, and a
        //    root tree that does not parse is refused while refusing is
        //    still free.
        let raw = self.superblock_image(blocks, commit)?;

        // 1. Every tree block, to every mirror. Nothing points at these
        //    yet, so their order among themselves does not matter --
        //    only that all of them precede the barrier.
        for block in blocks {
            if block.bytes.len() != self.sb.nodesize as usize {
                return Err(Error::UnsupportedFeature(format!(
                    "a tree block is {} bytes and the one for {} is {}",
                    self.sb.nodesize,
                    block.logical,
                    block.bytes.len()
                )));
            }
            Self::write_logical_all_mirrors(device, &self.map, block.logical, &block.bytes)?;
        }

        // Blocks at these addresses may be cached from before they were
        // freed and reused.
        self.forget_tree_blocks();

        // 2. The barrier. Everything above must be on the device before
        //    anything below reaches it, because what follows is the
        //    pointer to it.
        device.flush()?;

        // 3. The superblocks, in address order. Each copy carries its
        //    own address, so they are not identical images.
        for &offset in &SUPER_OFFSETS {
            if !self.superblock_copy_fits(offset) {
                continue;
            }
            let mut image = raw.clone();
            super_write::set_bytenr(&mut image, offset);
            super_write::stamp_checksum(&mut image, self.sb.csum_type);
            device.write_at(offset, &image)?;
        }

        // 4. The second flush -- the commit point. The superblock writes
        //    carry no FUA, so this is what makes them durable rather
        //    than merely issued. Without it the commit is not a commit.
        device.flush()?;
        Ok(())
    }

    /// Whether a superblock copy at `offset` is inside the device.
    ///
    /// The three offsets are fixed, and a small filesystem simply has
    /// fewer copies -- the traced commit wrote two, not three. Writing a
    /// copy past the end would either fail or, on a sparse file, silently
    /// extend it.
    fn superblock_copy_fits(&self, offset: u64) -> bool {
        offset + SUPERBLOCK_SIZE as u64 <= self.device.size_bytes()
    }

    /// The current superblock with `commit` applied.
    ///
    /// Read back from the device rather than re-encoded from the parsed
    /// struct: the superblock holds fields this driver does not model,
    /// and rebuilding it from what it understands would silently drop
    /// them.
    fn superblock_image(&self, blocks: &[PlacedBlock], commit: &Commit) -> Result<Vec<u8>> {
        let mut raw = vec![0u8; SUPERBLOCK_SIZE];
        self.device.read_at(SUPER_OFFSETS[0], &mut raw)?;
        super_write::apply(&mut raw, self.sb.csum_type, commit)?;
        let roots = self.backup_roots(&raw, blocks, commit)?;
        super_write::write_backup(&mut raw, commit.generation, &roots);
        super_write::stamp_checksum(&mut raw, self.sb.csum_type);
        Ok(raw)
    }

    /// The roots the commit leaves, for its backup slot (#78).
    ///
    /// The tree and chunk roots are in the superblock image. The extent,
    /// filesystem, device and checksum roots are `ROOT_ITEM`s in the new root
    /// tree, whose blocks are among `blocks` where the commit moved them and
    /// on the device where it did not.
    fn backup_roots(
        &self,
        raw: &[u8],
        blocks: &[PlacedBlock],
        commit: &Commit,
    ) -> Result<super_write::BackupRoots> {
        use crate::btree::{TreeBlock, TreeGeometry};
        use crate::superblock::offsets as so;
        use super_write::BackupRoot;
        let le64 = |at: usize| u64::from_le_bytes(raw[at..at + 8].try_into().expect("8 bytes"));
        let geom = TreeGeometry::from_superblock(&self.sb);
        let read = |logical: u64| -> Result<TreeBlock> {
            match blocks.iter().find(|b| b.logical == logical) {
                Some(b) => TreeBlock::parse(b.bytes.clone(), logical, &geom),
                None => self.read_tree_block(logical),
            }
        };

        const CSUM_TREE: u64 = 7;
        let mut found = std::collections::BTreeMap::new();
        let root = read(commit.root)?;
        let tree = BackupRoot {
            bytenr: commit.root,
            generation: commit.generation,
            level: root.header.level,
        };
        let mut stack = vec![root];
        // A root tree holds a few dozen items; a walk longer than the
        // filesystem has metadata blocks is a cycle, not a tree.
        let mut budget = self.sb.total_bytes / u64::from(self.sb.nodesize.max(1)) + 1;
        while let Some(block) = stack.pop() {
            budget = budget.checked_sub(1).ok_or_else(|| {
                Error::UnsupportedFeature("the root tree being committed loops".into())
            })?;
            if let Some(ptrs) = block.body.key_ptrs() {
                for p in ptrs {
                    stack.push(read(p.blockptr)?);
                }
                continue;
            }
            for item in block.body.items().unwrap_or_default() {
                let id = item.key.objectid;
                if item.key.key_type != crate::fs::ROOT_ITEM_KEY
                    || ![
                        objectid::EXTENT_TREE,
                        objectid::FS_TREE,
                        objectid::DEV_TREE,
                        CSUM_TREE,
                    ]
                    .contains(&id)
                {
                    continue;
                }
                let Some(data) = block.item_data(item).filter(|d| d.len() > root_item::LEVEL)
                else {
                    continue;
                };
                let at = |o: usize| u64::from_le_bytes(data[o..o + 8].try_into().expect("8 bytes"));
                found.insert(
                    id,
                    BackupRoot {
                        bytenr: at(root_item::BYTENR),
                        generation: at(root_item::GENERATION),
                        level: data[root_item::LEVEL],
                    },
                );
            }
        }
        let get = |id: u64| found.get(&id).copied().unwrap_or_default();
        Ok(super_write::BackupRoots {
            tree,
            chunk: BackupRoot {
                bytenr: le64(so::CHUNK_ROOT),
                generation: le64(so::CHUNK_ROOT_GENERATION),
                level: raw[so::CHUNK_ROOT_LEVEL],
            },
            extent: get(objectid::EXTENT_TREE),
            fs: get(objectid::FS_TREE),
            dev: get(objectid::DEV_TREE),
            csum: get(CSUM_TREE),
            total_bytes: le64(so::TOTAL_BYTES),
            bytes_used: le64(so::BYTES_USED),
            num_devices: le64(so::NUM_DEVICES),
        })
    }
}
