//! Host-side XFS log repair planning: replay or conservatively clear the log.
//!
//! A dirty v5 XFS volume only becomes mountable when the metadata that
//! exists solely in the log is materialized first (a captured image showed
//! recently-created directory blocks as zeros on disk; clearing the log
//! alone loses them and still fails the mount-time LSN verifiers). The
//! repair therefore runs in two stages, both expressed as volume-relative
//! patches that the caller applies through its own write mapping (the
//! emulation COW overlay) — this module never writes anything itself:
//!
//! 1. **Replay** (`plan_log_repair`): every committed transaction's BUF /
//!    INODE / ICREATE items become metadata patches, in increasing record-LSN
//!    order. This path is retained for analysis and controlled replay views.
//! 2. **Log rewrite**: the log area is rewritten into the shape of a real
//!    clean-unmount log: a fully cycle-stamped region with the mkfs-style
//!    dummy unmount record in the last two basic blocks. Its cycle is newer
//!    than the superblock and every replayed record LSN; RHEL7 validates
//!    `sb_lsn` against the recovered current log LSN even for a clean log.
//!    The end placement makes the inferred head wrap to the next cycle and
//!    avoids RHEL7's partially-zeroed-log rule. The kernel then accepts the
//!    unmount record, so no recovery runs (`xlog_find_zeroed`/`xlog_find_head`/
//!    `xlog_check_unmount_rec`/`xlog_clear_stale_blocks` chain verified
//!    against fs/xfs/xfs_log_recover.c and iterated against the RHEL7
//!    3.10 backport's observed behavior).

use std::collections::HashSet;

use crate::log::replay::{
    finalize_replay, replay_log_snapshot, stamp_crc32c, stamp_metadata_crc, ReplayGeometry,
};
use crate::log::{
    assess_log_state, dummy_unmount_record, XfsLogError, XfsLogSnapshot, XfsLogState,
    XFS_LOG_MAX_SNAPSHOT_BYTES, XLOG_BASIC_BLOCK_SIZE,
};
use crate::reader::XFS_INODE_MAGIC;
use crate::XfsReader;

/// One volume-relative write of the repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsRepairPatch {
    pub offset: u64,
    pub bytes: Vec<u8>,
}

/// The repair plan: replay patches (if any committed transactions were
/// found) followed by the cleared-log rewrite of the whole log area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsLogClearPlan {
    pub patches: Vec<XfsRepairPatch>,
    pub log_offset: u64,
    pub log_bytes: u64,
    /// Committed transactions whose items were replayed into patches.
    pub replayed_transactions: u32,
    /// Retained for the IPC contract. A successful fail-closed plan has zero
    /// skipped items; unsupported replay semantics return `XfsLogError`.
    pub skipped_items: u32,
}

const UNMOUNT_RECORD_BLOCKS: usize = 2;
/// XFS allocation-group headers use filesystem disk addresses (512-byte
/// sectors), not filesystem-block addresses.  In particular, AGI is sector
/// 2 from the start of each allocation group even when the data block size is
/// 4 KiB or larger.
const XFS_DADDR_SIZE: u64 = 512;
const XFS_AGI_DADDR_SECTORS: u64 = 2;
const XFS_AGI_BYTES: usize = XFS_DADDR_SIZE as usize;
const XFS_AGI_MAGIC: u32 = 0x5841_4749;
const AGI_UNLINKED_OFFSET: usize = 40;
const AGI_UNLINKED_COUNT: usize = 64;
const AGI_UUID_OFFSET: usize = 296;
const AGI_CRC_OFFSET: usize = 312;
const AGI_LSN_OFFSET: usize = 320;
const INODE_NEXT_UNLINKED_OFFSET: usize = 96;
const MAX_UNLINKED_CHAIN_STEPS: u64 = 1_000_000;

/// Advance beyond every metadata/log LSN that remains visible after replay.
/// XFS reserves the log-header magic as an invalid cycle number.
fn clean_cycle(superblock_lsn: u64, max_record_lsn: u64) -> Result<u32, XfsLogError> {
    let latest = ((superblock_lsn.max(max_record_lsn)) >> 32) as u32;
    let mut cycle = latest
        .checked_add(1)
        .ok_or_else(|| XfsLogError::InvalidData("XFS log cycle cannot be advanced".into()))?;
    if cycle == crate::log::XLOG_HEADER_MAGIC_NUM {
        cycle = cycle.checked_add(1).ok_or_else(|| {
            XfsLogError::InvalidData("XFS log cycle cannot skip its reserved value".into())
        })?;
    }
    Ok(cycle.max(1))
}

impl XfsReader {
    /// Assess the internal log and, when it is dirty, plan the
    /// replay-then-rewrite repair. `Ok(None)` means the volume is clean and
    /// must not be written.
    pub fn plan_log_repair(&self) -> Result<Option<XfsLogClearPlan>, XfsLogError> {
        let snapshot = self.read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)?;
        if assess_log_state(&snapshot) != XfsLogState::Dirty {
            return Ok(None);
        }
        self.build_plan(&snapshot).map(Some)
    }

    /// Plan the conservative equivalent of `xfs_repair -L`: discard the
    /// captured log without replaying its transactions, then terminate the
    /// log with a valid clean-unmount record. This is intended for emulation
    /// of live-captured filesystems where logged inode cores can describe
    /// metadata that was not captured atomically with its data fork. Analysis
    /// callers should continue to use [`Self::plan_log_repair`] so that the
    /// forensic replay view remains available.
    pub fn plan_log_clear(&self) -> Result<Option<XfsLogClearPlan>, XfsLogError> {
        let snapshot = self.read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)?;
        if assess_log_state(&snapshot) != XfsLogState::Dirty {
            return Ok(None);
        }
        self.finish_clear_plan(&snapshot, 0, 0, Vec::new(), 0)
            .map(Some)
    }

    /// Verify the state produced by [`Self::plan_log_clear`]. This is
    /// intentionally stricter than log assessment: a misplaced AGI patch
    /// would leave the log clean while the guest still sees orphaned inodes.
    pub fn verify_log_clear(&self) -> Result<(), XfsLogError> {
        let snapshot = self.read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)?;
        if assess_log_state(&snapshot) != XfsLogState::Clean {
            return Err(XfsLogError::InvalidData(
                "cleared XFS log is not assessed as clean".into(),
            ));
        }
        self.verify_ag_unlinked_clear()
    }

    fn replay_geometry(&self) -> ReplayGeometry {
        ReplayGeometry {
            block_size: self.block_size,
            dblocks: self.dblocks,
            ag_blocks: self._ag_blocks,
            ag_count: self._ag_count,
            inode_size: self.inode_size,
            inopblog: self.inopblog,
            agblklog: self.agblklog,
            metadata_uuid: self.metadata_uuid,
        }
    }

    fn build_plan(&self, snapshot: &XfsLogSnapshot) -> Result<XfsLogClearPlan, XfsLogError> {
        let geometry = self.log_geometry();
        let basic_blocks = usize::try_from(geometry.basic_block_count()?)
            .map_err(|_| XfsLogError::InvalidGeometry("log basic-block count overflows".into()))?;
        if basic_blocks < 1024 {
            return Err(XfsLogError::InvalidGeometry(
                "log too small for the stamped clear layout".into(),
            ));
        }
        let replay = replay_log_snapshot(snapshot, &self.replay_geometry())?;
        let max_record_lsn = replay.max_record_lsn;
        let replayed_transactions = replay.replayed_transactions;
        let finalized = finalize_replay(
            replay,
            geometry.metadata_crc,
            &self.metadata_uuid,
            |offset, length| {
                let absolute = self.volume_offset.checked_add(offset).ok_or_else(|| {
                    XfsLogError::InvalidData("replay read offset overflows".into())
                })?;
                self.read_bytes_at(absolute, length)
                    .map_err(XfsLogError::Io)
            },
        )?;

        self.finish_clear_plan(
            snapshot,
            max_record_lsn,
            replayed_transactions,
            finalized.patches,
            finalized.skipped_items,
        )
    }

    fn finish_clear_plan(
        &self,
        snapshot: &XfsLogSnapshot,
        max_record_lsn: u64,
        replayed_transactions: u32,
        finalized_patches: Vec<crate::log::replay::XfsReplayPatch>,
        skipped_items: u32,
    ) -> Result<XfsLogClearPlan, XfsLogError> {
        let geometry = self.log_geometry();
        let log_bytes = geometry.log_bytes()?;
        let basic_blocks = usize::try_from(geometry.basic_block_count()?)
            .map_err(|_| XfsLogError::InvalidGeometry("log basic-block count overflows".into()))?;
        if basic_blocks < 1024 {
            return Err(XfsLogError::InvalidGeometry(
                "log too small for the stamped clear layout".into(),
            ));
        }
        let cycle = clean_cycle(self.superblock_lsn, max_record_lsn)?;

        // RHEL7 rejects a partially zeroed log unless its first cycle is 1.
        // Stamp the complete region and terminate it at the physical end.
        // find_head then returns 0, the backward record search wraps and
        // advances the in-core cycle, and the record ends exactly at head.
        let mut image = vec![0u8; basic_blocks * XLOG_BASIC_BLOCK_SIZE];
        for block in 0..basic_blocks {
            image[block * XLOG_BASIC_BLOCK_SIZE..block * XLOG_BASIC_BLOCK_SIZE + 4]
                .copy_from_slice(&cycle.to_be_bytes());
        }
        let record_block = basic_blocks - UNMOUNT_RECORD_BLOCKS;
        let record = dummy_unmount_record(
            &geometry.fs_uuid,
            geometry.record_version,
            cycle,
            record_block as u64,
            record_block as u64,
        );
        image[record_block * XLOG_BASIC_BLOCK_SIZE..basic_blocks * XLOG_BASIC_BLOCK_SIZE]
            .copy_from_slice(&record);

        let mut patches: Vec<XfsRepairPatch> = finalized_patches
            .into_iter()
            .map(|patch| XfsRepairPatch {
                offset: patch.offset,
                bytes: patch.bytes,
            })
            .collect();
        patches.extend(self.plan_ag_unlinked_clear()?);
        patches.push(XfsRepairPatch {
            offset: snapshot.source_offset,
            bytes: image,
        });

        Ok(XfsLogClearPlan {
            patches,
            log_offset: snapshot.source_offset,
            log_bytes,
            replayed_transactions,
            skipped_items,
        })
    }

    /// Discard AGI unlinked-list state along with the dirty log. A live image
    /// can contain list pointers to inode cores captured at different times;
    /// replaying those pointers is precisely the state that `xfs_repair -L`
    /// removes before rebuilding allocation trees. The tree rebuild itself is
    /// intentionally outside this reader's proof boundary.
    fn plan_ag_unlinked_clear(&self) -> Result<Vec<XfsRepairPatch>, XfsLogError> {
        let mut patches = Vec::new();
        for ag in 0..self._ag_count {
            let ag_offset = self.agi_offset(ag)?;
            let mut block = self
                .read_bytes_at(ag_offset, XFS_AGI_BYTES)
                .map_err(XfsLogError::Io)?;
            if block.len() < AGI_UNLINKED_OFFSET + AGI_UNLINKED_COUNT * 4
                || crate::be_u32(&block, 0) != XFS_AGI_MAGIC
            {
                continue;
            }
            if self.log_geometry.metadata_crc
                && (block.len() < AGI_LSN_OFFSET + 8
                    || block.get(AGI_UUID_OFFSET..AGI_UUID_OFFSET + 16)
                        != Some(self.metadata_uuid.as_slice()))
            {
                return Err(XfsLogError::InvalidData(format!(
                    "AGI block for allocation group {ag} has a foreign UUID"
                )));
            }
            patches.extend(self.plan_unlinked_inode_clear(ag, &block)?);
            let mut changed = false;
            for index in 0..AGI_UNLINKED_COUNT {
                let offset = AGI_UNLINKED_OFFSET + index * 4;
                if block[offset..offset + 4] != u32::MAX.to_be_bytes() {
                    block[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
                    changed = true;
                }
            }
            if self.log_geometry.metadata_crc && block[AGI_LSN_OFFSET..AGI_LSN_OFFSET + 8] != [0; 8]
            {
                block[AGI_LSN_OFFSET..AGI_LSN_OFFSET + 8].fill(0);
                changed = true;
            }
            if !changed {
                continue;
            }
            if self.log_geometry.metadata_crc {
                stamp_crc32c(&mut block, AGI_CRC_OFFSET);
            }
            let offset = ag_offset.checked_sub(self.volume_offset).ok_or_else(|| {
                XfsLogError::InvalidData("AGI patch precedes the XFS volume".into())
            })?;
            patches.push(XfsRepairPatch {
                offset,
                bytes: block,
            });
        }
        Ok(patches)
    }

    fn agi_offset(&self, ag: u32) -> Result<u64, XfsLogError> {
        u64::from(ag)
            .checked_mul(self._ag_blocks)
            .and_then(|blocks| blocks.checked_mul(self.block_size))
            .and_then(|offset| offset.checked_add(XFS_AGI_DADDR_SECTORS * XFS_DADDR_SIZE))
            .ok_or_else(|| XfsLogError::InvalidGeometry("AGI offset overflows".into()))
    }

    fn verify_ag_unlinked_clear(&self) -> Result<(), XfsLogError> {
        for ag in 0..self._ag_count {
            let offset = self.agi_offset(ag)?;
            let block = self
                .read_bytes_at(offset, XFS_AGI_BYTES)
                .map_err(XfsLogError::Io)?;
            if block.len() < AGI_UNLINKED_OFFSET + AGI_UNLINKED_COUNT * 4
                || crate::be_u32(&block, 0) != XFS_AGI_MAGIC
            {
                continue;
            }
            if block[AGI_UNLINKED_OFFSET..AGI_UNLINKED_OFFSET + AGI_UNLINKED_COUNT * 4]
                .chunks_exact(4)
                .any(|value| value != u32::MAX.to_be_bytes())
            {
                return Err(XfsLogError::InvalidData(format!(
                    "AGI block for allocation group {ag} still contains an unlinked inode"
                )));
            }
            if self.log_geometry.metadata_crc && !agi_metadata_crc_is_valid(&block) {
                return Err(XfsLogError::InvalidData(format!(
                    "AGI block for allocation group {ag} has an invalid CRC"
                )));
            }
        }
        Ok(())
    }

    /// Clear the inode-side links reachable from the AGI buckets. Once the
    /// buckets themselves are discarded, retaining a `di_next_unlinked` value
    /// leaves xfs_repair to find a dangling orphan on the next boot.
    fn plan_unlinked_inode_clear(
        &self,
        ag: u32,
        agi: &[u8],
    ) -> Result<Vec<XfsRepairPatch>, XfsLogError> {
        let inode_capacity = self
            ._ag_blocks
            .checked_shl(u32::from(self.inopblog))
            .ok_or_else(|| XfsLogError::InvalidGeometry("AG inode capacity overflows".into()))?;
        let mut patches = Vec::new();
        let mut visited = HashSet::new();
        for bucket in 0..AGI_UNLINKED_COUNT {
            let offset = AGI_UNLINKED_OFFSET + bucket * 4;
            let head = crate::be_u32(agi, offset);
            if head == u32::MAX || u64::from(head) >= inode_capacity {
                continue;
            }
            let mut agino = head;
            let mut steps = 0u64;
            while agino != u32::MAX
                && u64::from(agino) < inode_capacity
                && steps < MAX_UNLINKED_CHAIN_STEPS
                && visited.insert(agino)
            {
                let inode_number = self
                    .inode_number_for_agino(ag, agino)
                    .map_err(XfsLogError::InvalidGeometry)?;
                let mut inode = match self.read_inode(inode_number) {
                    Ok(inode) => inode,
                    Err(_) => break,
                };
                if inode.len() < INODE_NEXT_UNLINKED_OFFSET + 4
                    || !matches!(inode.get(crate::di_off::VERSION), Some(1..=3))
                    || crate::be_u16(&inode, crate::di_off::MAGIC) != XFS_INODE_MAGIC
                {
                    break;
                }
                let next = crate::be_u32(&inode, INODE_NEXT_UNLINKED_OFFSET);
                if next != u32::MAX {
                    inode[INODE_NEXT_UNLINKED_OFFSET..INODE_NEXT_UNLINKED_OFFSET + 4]
                        .copy_from_slice(&u32::MAX.to_be_bytes());
                    if self.log_geometry.metadata_crc && inode[crate::di_off::VERSION] == 3 {
                        stamp_metadata_crc(&mut inode);
                    }
                    let offset = self
                        .inode_offset(inode_number)
                        .map_err(XfsLogError::Io)?
                        .checked_sub(self.volume_offset)
                        .ok_or_else(|| {
                            XfsLogError::InvalidData(
                                "unlinked inode patch precedes the XFS volume".into(),
                            )
                        })?;
                    patches.push(XfsRepairPatch {
                        offset,
                        bytes: inode,
                    });
                }
                agino = next;
                steps += 1;
            }
        }
        Ok(patches)
    }

    fn inode_number_for_agino(&self, ag: u32, agino: u32) -> Result<u64, String> {
        let shift = self
            .agblklog
            .checked_add(self.inopblog)
            .ok_or_else(|| "XFS inode number shift overflows".to_string())?;
        if shift >= u64::BITS as u8 {
            return Err("XFS inode number shift is invalid".to_string());
        }
        u64::from(ag)
            .checked_shl(u32::from(shift))
            .and_then(|base| base.checked_add(u64::from(agino)))
            .ok_or_else(|| "XFS inode number overflows".to_string())
    }
}

fn agi_metadata_crc_is_valid(object: &[u8]) -> bool {
    if object.get(AGI_CRC_OFFSET..AGI_CRC_OFFSET + 4).is_none() {
        return false;
    }
    let mut sealed = object.to_vec();
    stamp_crc32c(&mut sealed, AGI_CRC_OFFSET);
    sealed[AGI_CRC_OFFSET..AGI_CRC_OFFSET + 4] == object[AGI_CRC_OFFSET..AGI_CRC_OFFSET + 4]
}
