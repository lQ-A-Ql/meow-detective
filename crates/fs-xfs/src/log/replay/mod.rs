//! Host-side replay of committed XFS log transactions.
//!
//! This is the read-only-evidence equivalent of the kernel's mount-time log
//! recovery (`xlog_recover`, fs/xfs/xfs_log_recover.c plus the per-item
//! `commit_pass2` handlers): committed transactions are reassembled from the
//! CRC-validated records and their BUF / INODE / ICREATE items are turned
//! into volume-relative write patches. Nothing is written here; the caller
//! applies the patches through its own copy-on-write overlay.
//!
//! Deliberate deviations from the kernel, all biased towards producing a
//! consistent read view instead of failing the mount:
//!
//! - Items the kernel would abort recovery on (truncated forks, unknown
//!   types, out-of-geometry writes, b-tree-root conversions) are skipped and
//!   counted instead of failing the whole replay.
//! - The kernel's on-disk LSN read-back skips are reproduced for v5 BUF and
//!   INODE items. Grouped actions are finalized against the current volume
//!   image, and recovery write verifiers reseal complete metadata objects.
//! - The buffer cancellation table (`XFS_BLF_CANCEL`) IS reproduced, because
//!   replaying a freed-and-reused buffer would corrupt user data.
//! - INODE items carrying `XFS_ILOG_DBROOT`/`XFS_ILOG_ABROOT` need the
//!   `xfs_bmbt_to_bmdr` root conversion; that is not implemented and such
//!   items are skipped.

mod assemble;
mod buffer;
mod dinode;
mod finalize;
mod items;
mod sink;

use super::{XfsLogError, XfsLogSnapshot};
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

/// The result of replaying every committed transaction in a log snapshot.
#[derive(Debug, Default)]
pub(crate) struct XfsLogReplay {
    pub(crate) actions: Vec<XfsReplayAction>,
    /// Greatest CRC-validated record LSN observed in the source log.
    pub(crate) max_record_lsn: u64,
    /// Committed transactions that carried at least one complete item.
    pub(crate) replayed_transactions: u32,
    /// Items skipped because their semantics could not be reproduced:
    /// unknown types, malformed payloads, out-of-geometry writes, or
    /// unsupported conversions. Cancellation-table suppressions are handled
    /// recovery actions and are not counted here.
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
    let collection =
        super::record::collect_log_records(snapshot, MAX_REPLAY_RECORDS, MAX_REPLAY_BODY_BYTES)?;
    let max_record_lsn = collection
        .records
        .iter()
        .map(|record| record.header.lsn)
        .max()
        .unwrap_or(0);
    let assembly = assemble::assemble_committed(&collection.records)?;
    let outcome = items::apply_transactions(geometry, &assembly.transactions)?;
    Ok(XfsLogReplay {
        actions: outcome.actions,
        max_record_lsn,
        replayed_transactions: u32::try_from(assembly.transactions.len()).unwrap_or(u32::MAX),
        skipped_items: outcome.skipped_items.saturating_add(assembly.dropped_items),
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/log_replay.rs"]
mod tests;
