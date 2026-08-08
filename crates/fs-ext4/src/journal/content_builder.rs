use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::content::{
    DeletedContentMapping, DeletedContentMappingState, DeletedContentRange,
    DeletedContentRangeKind, RecoveryAllocationState,
};
use super::error::{JournalError, JournalResult};

pub(super) struct MappingBuilder {
    inode_allocation_state: RecoveryAllocationState,
    ranges: Vec<DeletedContentRange>,
    recoverable_bytes: u64,
    saw_free: bool,
    saw_allocated: bool,
    content_md5: Md5,
    content_sha1: Sha1,
    content_sha256: Sha256,
    hashed_bytes: u64,
    content_hash_valid: bool,
}

impl MappingBuilder {
    pub(super) fn new(inode_allocation_state: RecoveryAllocationState) -> Self {
        Self {
            inode_allocation_state,
            ranges: Vec::new(),
            recoverable_bytes: 0,
            saw_free: false,
            saw_allocated: false,
            content_md5: Md5::new(),
            content_sha1: Sha1::new(),
            content_sha256: Sha256::new(),
            hashed_bytes: 0,
            content_hash_valid: true,
        }
    }

    pub(super) fn observe_allocation(&mut self, allocation: RecoveryAllocationState) {
        self.saw_free |= allocation == RecoveryAllocationState::Free;
        self.saw_allocated |= allocation == RecoveryAllocationState::Allocated;
    }

    pub(super) fn observe_recoverable_content(
        &mut self,
        logical_offset: u64,
        bytes: &[u8],
    ) -> JournalResult<()> {
        if logical_offset != self.hashed_bytes {
            self.content_hash_valid = false;
            return Ok(());
        }
        self.content_md5.update(bytes);
        self.content_sha1.update(bytes);
        self.content_sha256.update(bytes);
        self.hashed_bytes = self
            .hashed_bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| JournalError::Invalid("content hash byte count overflows".into()))?;
        Ok(())
    }

    pub(super) fn push_sparse(&mut self, logical_offset: u64, length: u64) -> JournalResult<()> {
        self.push_range(DeletedContentRange {
            logical_offset,
            filesystem_block: None,
            filesystem_source_offset: None,
            length,
            kind: DeletedContentRangeKind::Sparse,
            allocation_state: RecoveryAllocationState::Unverified,
            sha256: None,
        })
    }

    pub(super) fn push_range(&mut self, range: DeletedContentRange) -> JournalResult<()> {
        if range.length == 0 {
            return Err(JournalError::Invalid(
                "deleted-content range length is zero".into(),
            ));
        }
        if range.kind == DeletedContentRangeKind::RecoverableData
            && self.inode_allocation_state == RecoveryAllocationState::Free
        {
            self.recoverable_bytes = self
                .recoverable_bytes
                .checked_add(range.length)
                .ok_or_else(|| JournalError::Invalid("recoverable byte count overflows".into()))?;
        }
        if range.kind != DeletedContentRangeKind::RecoverableData {
            self.content_hash_valid = false;
        }
        if let Some(previous) = self.ranges.last_mut() {
            let logical_contiguous = previous
                .logical_offset
                .checked_add(previous.length)
                .is_some_and(|end| end == range.logical_offset);
            let compatible_block_provenance = matches!(
                (previous.filesystem_block, range.filesystem_block),
                (Some(_), Some(_)) | (None, None)
            );
            let source_contiguous = match (
                previous.filesystem_source_offset,
                range.filesystem_source_offset,
            ) {
                (Some(previous_source), Some(current)) => previous_source
                    .checked_add(previous.length)
                    .is_some_and(|end| end == current),
                (None, None) => true,
                _ => false,
            };
            if logical_contiguous
                && compatible_block_provenance
                && source_contiguous
                && previous.kind == range.kind
                && previous.allocation_state == range.allocation_state
                && previous.sha256.is_none()
                && range.sha256.is_none()
            {
                previous.length = previous.length.checked_add(range.length).ok_or_else(|| {
                    JournalError::Invalid("content range length overflows".into())
                })?;
                return Ok(());
            }
        }
        self.ranges.push(range);
        Ok(())
    }

    pub(super) fn finish(self) -> DeletedContentMapping {
        let data_allocation_state = match (self.saw_free, self.saw_allocated) {
            (true, true) => RecoveryAllocationState::Mixed,
            (true, false) => RecoveryAllocationState::Free,
            (false, true) => RecoveryAllocationState::Allocated,
            (false, false) => RecoveryAllocationState::Unverified,
        };
        let has_complete_hashes = self.content_hash_valid
            && self.inode_allocation_state == RecoveryAllocationState::Free
            && self.hashed_bytes == self.recoverable_bytes;
        let content_md5 = has_complete_hashes.then(|| hex::encode(self.content_md5.finalize()));
        let content_sha1 = has_complete_hashes.then(|| hex::encode(self.content_sha1.finalize()));
        let content_sha256 =
            has_complete_hashes.then(|| hex::encode(self.content_sha256.finalize()));
        DeletedContentMapping {
            state: DeletedContentMappingState::Mapped,
            inode_allocation_state: self.inode_allocation_state,
            data_allocation_state,
            ranges: self.ranges,
            recoverable_bytes: self.recoverable_bytes,
            content_md5,
            content_sha1,
            content_sha256,
            issue: None,
        }
    }
}
