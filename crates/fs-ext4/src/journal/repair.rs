//! Offline jbd2 replay planning for emulation overlays.
//!
//! The planner mirrors the recovery part of an ext4 mount: committed
//! transactions are copied to their filesystem blocks in sequence order, then
//! the journal superblock is checkpointed by clearing `s_start`. No evidence
//! bytes are written here; the caller owns patch application and verification.

use super::checksum::crc32c_with_zeroed_range;
use super::error::{JournalError, JournalResult};
use super::ring::{journal_block_data, parse_journal};
use super::types::{JBD2_FEATURE_INCOMPAT_FAST_COMMIT, JBD2_FLAG_ESCAPE, JOURNAL_SUPERBLOCK_SIZE};
use crate::Ext4FileExtent;
use crate::Ext4Reader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ext4JournalRepairPatch {
    pub volume_offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ext4JournalRepairPlan {
    pub patches: Vec<Ext4JournalRepairPatch>,
    pub journal_bytes: u64,
    pub transactions_replayed: u32,
}

impl Ext4Reader {
    /// Builds an overlay-only recovery plan for the internal journal.
    ///
    /// `Ok(None)` means the filesystem has no internal journal or its journal
    /// is already checkpointed. Incomplete transactions are intentionally
    /// discarded, matching mount-time recovery semantics. Fast-commit state
    /// is refused when active because this parser does not yet replay its
    /// compact records.
    pub fn plan_journal_repair(
        &self,
        max_bytes: usize,
    ) -> JournalResult<Option<Ext4JournalRepairPlan>> {
        if !self.has_journal {
            return Ok(None);
        }
        let journal = self.read_internal_journal(max_bytes)?;
        let scan = parse_journal(&journal)?;
        if scan.superblock.start == 0 {
            return Ok(None);
        }
        if scan
            .superblock
            .has_incompat(JBD2_FEATURE_INCOMPAT_FAST_COMMIT)
        {
            return Err(JournalError::Unsupported(
                "ext4 fast-commit records are not supported by offline journal replay".into(),
            ));
        }

        let mut patches = Vec::new();
        for transaction in &scan.transactions {
            for mapping in &transaction.mappings {
                if mapping.revoked {
                    continue;
                }
                let payload =
                    journal_block_data(&journal, &scan.superblock, mapping.payload_journal_block)?;
                let payload = unescaped_payload(payload, mapping.flags);
                let volume_offset = self
                    .block_to_offset(mapping.target_filesystem_block)
                    .map_err(JournalError::Io)?;
                patches.push(Ext4JournalRepairPatch {
                    volume_offset,
                    bytes: payload,
                });
            }
        }

        // Linux computes jbd2_superblock_t's CRC over its fixed 0x400-byte
        // struct, even when the journal block size is 4 KiB or larger. The
        // bytes after this structure belong to the journal block padding and
        // are not part of the checksum.
        let mut superblock = journal[..JOURNAL_SUPERBLOCK_SIZE].to_vec();
        superblock[0x1C..0x20].copy_from_slice(&0u32.to_be_bytes());
        if scan.superblock.uses_v2_or_v3_checksums() {
            let checksum = crc32c_with_zeroed_range(u32::MAX, &superblock, 0xFC..0x100)
                .ok_or_else(|| JournalError::Invalid("journal checksum range is invalid".into()))?;
            superblock[0xFC..0x100].copy_from_slice(&checksum.to_be_bytes());
        }
        let journal_extents = self.internal_journal_extent_map()?;
        patches.extend(map_journal_range(&journal_extents, 0, &superblock)?);

        Ok(Some(Ext4JournalRepairPlan {
            patches,
            journal_bytes: journal.len() as u64,
            transactions_replayed: scan.transactions.len() as u32,
        }))
    }

    /// Verifies that a journal no longer advertises transactions for replay.
    pub fn verify_journal_repair(&self) -> JournalResult<()> {
        if self.journal_replay_required()? {
            return Err(JournalError::Invalid(
                "ext4 journal still advertises transactions after repair".into(),
            ));
        }
        Ok(())
    }
}

fn unescaped_payload(payload: &[u8], flags: u32) -> Vec<u8> {
    if flags & JBD2_FLAG_ESCAPE == 0 || payload.len() < 4 {
        return payload.to_vec();
    }
    let mut restored = payload.to_vec();
    restored[..4].copy_from_slice(&super::types::JBD2_MAGIC_NUMBER.to_be_bytes());
    restored
}

fn map_journal_range(
    extents: &[Ext4FileExtent],
    logical_start: u64,
    bytes: &[u8],
) -> JournalResult<Vec<Ext4JournalRepairPatch>> {
    let logical_end = logical_start
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| JournalError::Invalid("journal patch range overflows".into()))?;
    let mut patches = Vec::new();
    let mut cursor = logical_start;
    for extent in extents {
        let extent_end = extent
            .logical_offset
            .checked_add(extent.length)
            .ok_or_else(|| JournalError::Invalid("journal extent range overflows".into()))?;
        if extent_end <= cursor || extent.logical_offset >= logical_end {
            continue;
        }
        if extent.logical_offset > cursor {
            return Err(JournalError::Unsupported(
                "internal journal superblock maps through a sparse extent gap".into(),
            ));
        }
        let start = cursor.max(extent.logical_offset);
        let end = logical_end.min(extent_end);
        let source_start = usize::try_from(start - logical_start).map_err(|_| {
            JournalError::Unsupported("journal patch exceeds addressable memory".into())
        })?;
        let source_end = usize::try_from(end - logical_start).map_err(|_| {
            JournalError::Unsupported("journal patch exceeds addressable memory".into())
        })?;
        let volume_offset = extent
            .volume_offset
            .checked_add(start - extent.logical_offset)
            .ok_or_else(|| JournalError::Invalid("journal patch offset overflows".into()))?;
        patches.push(Ext4JournalRepairPatch {
            volume_offset,
            bytes: bytes[source_start..source_end].to_vec(),
        });
        cursor = end;
        if cursor == logical_end {
            break;
        }
    }
    if cursor != logical_end {
        return Err(JournalError::Unsupported(
            "internal journal superblock extent map is incomplete".into(),
        ));
    }
    Ok(patches)
}
