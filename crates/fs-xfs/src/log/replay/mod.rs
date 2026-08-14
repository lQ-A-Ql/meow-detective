//! Host-side replay of committed XFS log transactions.
//!
//! This is the read-only-evidence equivalent of the kernel's mount-time log
//! recovery (`xlog_recover`, fs/xfs/xfs_log_recover.c plus the per-item
//! `commit_pass2` handlers): committed transactions are reassembled from the
//! CRC-validated records and their BUF / INODE / ICREATE items are turned
//! into volume-relative write patches. Nothing is written here; the caller
//! applies the patches through its own copy-on-write overlay.
//!
//! The planner is deliberately fail-closed: a clean-log rewrite is emitted
//! only when every committed item is either reproduced or proven harmless by
//! the recovery rules. Unknown/truncated items and unsupported metadata
//! transformations abort planning.
//!
//! - The kernel's on-disk LSN read-back skips are reproduced for v5 BUF and
//!   INODE items. Grouped actions are finalized against the current volume
//!   image, and recovery write verifiers reseal complete metadata objects.
//! - The buffer cancellation table (`XFS_BLF_CANCEL`) IS reproduced, because
//!   replaying a freed-and-reused buffer would corrupt user data.
//! - EFI/EFD intent IDs are paired in log order. An EFD without a matching
//!   EFI is harmless (as in the kernel); an EFI left pending would require
//!   post-recovery extent freeing and therefore aborts host-side planning.

mod active;
mod assemble;
mod buffer;
mod deferred;
mod dinode;
mod finalize;
mod icreate;
mod inode_buffer;
mod items;
mod sink;

use super::{XfsLogError, XfsLogIssueKind, XfsLogSnapshot};
pub(crate) use finalize::finalize_replay;

/// Hard cap on committed transactions replayed from one log.
pub(crate) const MAX_REPLAY_TRANSACTIONS: usize = 100_000;
/// Hard cap on the total bytes of replay patches.
pub(crate) const MAX_REPLAY_PATCH_BYTES: u64 = 512 * 1024 * 1024;
/// Record-collection bounds for the replay scan.
const MAX_REPLAY_RECORDS: usize = 100_000;
const MAX_REPLAY_BODY_BYTES: u64 = 512 * 1024 * 1024;

/// Filesystem geometry the replay needs to translate and bound item writes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReplayGeometry {
    pub(crate) block_size: u64,
    pub(crate) dblocks: u64,
    pub(crate) ag_blocks: u64,
    pub(crate) ag_count: u32,
    pub(crate) inode_size: u16,
    pub(crate) inopblog: u8,
    pub(crate) agblklog: u8,
    pub(crate) metadata_uuid: [u8; 16],
}

impl ReplayGeometry {
    fn capacity(&self) -> Result<u64, XfsLogError> {
        self.dblocks
            .checked_mul(self.block_size)
            .ok_or_else(|| XfsLogError::InvalidGeometry("filesystem capacity overflows".into()))
    }
}

/// One volume-relative write produced by the replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XfsReplayPatch {
    pub(crate) offset: u64,
    pub(crate) bytes: Vec<u8>,
}

/// One physical buffer item before the recovery-time LSN check and write
/// verifier are applied. Logged regions are kept grouped so the planner can
/// read the current buffer image and make the same skip decision as XFS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XfsBufferReplay {
    pub(crate) offset: u64,
    pub(crate) length: usize,
    pub(crate) lsn: u64,
    pub(crate) buffer_type: u16,
    pub(crate) inode_unlinked_only: bool,
    pub(crate) inode_size: u16,
    pub(crate) ag_inode_count: u64,
    pub(crate) writes: Vec<XfsReplayPatch>,
}

/// One logical inode item before the current on-disk `di_lsn` check. The
/// core and fork writes are finalized together because `di_crc` covers the
/// complete on-disk inode, not only the 176-byte v3 core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XfsInodeReplay {
    pub(crate) offset: u64,
    pub(crate) length: usize,
    pub(crate) lsn: u64,
    pub(crate) inode_number: u64,
    pub(crate) writes: Vec<XfsReplayPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum XfsReplayAction {
    Patch(XfsReplayPatch),
    Buffer(XfsBufferReplay),
    Inode(XfsInodeReplay),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub(crate) enum ReplayDisposition {
    Applied,
    AlreadyCurrent,
    Cancelled,
    DeferredResolved,
}

/// The result of replaying every committed transaction in a log snapshot.
#[derive(Debug, Default)]
pub(crate) struct XfsLogReplay {
    pub(crate) actions: Vec<XfsReplayAction>,
    /// Greatest CRC-validated record LSN observed in the source log.
    pub(crate) max_record_lsn: u64,
    /// Committed transactions that carried at least one complete item.
    pub(crate) replayed_transactions: u32,
    /// Retained for the public repair-plan contract. Fail-closed replay never
    /// returns a successful plan with skipped items.
    pub(crate) skipped_items: u32,
}

#[derive(Debug, Default)]
pub(crate) struct XfsReplayFinal {
    pub(crate) patches: Vec<XfsReplayPatch>,
    pub(crate) skipped_items: u32,
}

/// Replay every committed transaction of the snapshot, in increasing
/// record-LSN order (the kernel's tail-to-head pass over a single wrap).
pub(crate) fn replay_log_snapshot(
    snapshot: &XfsLogSnapshot,
    geometry: &ReplayGeometry,
) -> Result<XfsLogReplay, XfsLogError> {
    if !snapshot.complete {
        return Err(XfsLogError::UnsafeReplay(
            "XFS repair requires a complete internal log snapshot".into(),
        ));
    }
    let collection =
        super::record::collect_log_records(snapshot, MAX_REPLAY_RECORDS, MAX_REPLAY_BODY_BYTES)?;
    if collection
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::LimitReached)
    {
        return Err(XfsLogError::UnsafeReplay(
            "XFS active-log discovery reached a replay scan limit".into(),
        ));
    }
    let total_blocks = u32::try_from(snapshot.geometry.basic_block_count()?).map_err(|_| {
        XfsLogError::InvalidGeometry("log basic-block count exceeds replay limits".into())
    })?;
    let records = active::select_active_records(collection.records, total_blocks)?;
    let max_record_lsn = records
        .iter()
        .map(|record| record.header.lsn)
        .max()
        .unwrap_or(0);
    let assembly = assemble::assemble_committed(&records)?;
    let outcome = items::apply_transactions(geometry, &assembly.transactions)?;
    Ok(XfsLogReplay {
        actions: outcome.actions,
        max_record_lsn,
        replayed_transactions: u32::try_from(assembly.transactions.len()).unwrap_or(u32::MAX),
        skipped_items: 0,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/log_replay.rs"]
mod tests;
