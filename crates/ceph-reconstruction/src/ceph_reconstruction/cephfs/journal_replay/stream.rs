use sha2::{Digest, Sha256};
use thiserror::Error;

use ceph_wire::{format_cephfs_journal_data_object_name, plan_cephfs_journal_range};

use super::super::{
    validate_range_response, CephFsDescriptor, CephFsInventoryError, CephFsObjectLocator,
    CephFsObjectRangeReader, CephFsObjectReadError, MAX_CEPHFS_OBJECT_RANGE_LENGTH,
};
use super::CephFsJournalSourceSpan;

pub(super) struct JournalStreamReader<'a, R> {
    descriptor: &'a CephFsDescriptor,
    rank: u32,
    journal_inode: u64,
    layout: ceph_wire::CephFsJournalLayout,
    reader: &'a mut R,
}

pub(super) struct JournalRead {
    pub bytes: Vec<u8>,
    pub spans: Vec<CephFsJournalSourceSpan>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum JournalStreamError {
    #[error("invalid CephFS journal stream object mapping")]
    InvalidMapping,
    #[error(transparent)]
    Inventory(#[from] CephFsInventoryError),
    #[error(transparent)]
    Object(#[from] CephFsObjectReadError),
    #[error(transparent)]
    Wire(#[from] ceph_wire::CephWireError),
}

impl<'a, R: CephFsObjectRangeReader> JournalStreamReader<'a, R> {
    pub fn new(
        descriptor: &'a CephFsDescriptor,
        rank: u32,
        journal_inode: u64,
        layout: ceph_wire::CephFsJournalLayout,
        reader: &'a mut R,
    ) -> Self {
        Self {
            descriptor,
            rank,
            journal_inode,
            layout,
            reader,
        }
    }

    pub fn read_exact(
        &mut self,
        logical_offset: u64,
        length: usize,
    ) -> Result<JournalRead, JournalStreamError> {
        let extents = plan_cephfs_journal_range(self.layout, logical_offset, length)?;
        let mut bytes = Vec::with_capacity(length);
        let mut spans = Vec::new();
        for extent in extents {
            self.read_extent(extent, &mut bytes, &mut spans)?;
        }
        if bytes.len() != length {
            return Err(JournalStreamError::InvalidMapping);
        }
        Ok(JournalRead { bytes, spans })
    }

    fn read_extent(
        &mut self,
        extent: ceph_wire::CephFsJournalObjectExtent,
        bytes: &mut Vec<u8>,
        spans: &mut Vec<CephFsJournalSourceSpan>,
    ) -> Result<(), JournalStreamError> {
        let object_name = format_cephfs_journal_data_object_name(
            self.rank,
            self.journal_inode,
            extent.object_index,
        )
        .ok_or(JournalStreamError::InvalidMapping)?;
        let locator = CephFsObjectLocator::new(
            self.descriptor.filesystem_id,
            self.descriptor.metadata_pool.pool_id,
            Vec::new(),
            object_name.into_bytes(),
            self.descriptor.fsmap_epoch,
        )?;
        let mut consumed = 0usize;
        while consumed < extent.length {
            let chunk_length = (extent.length - consumed).min(MAX_CEPHFS_OBJECT_RANGE_LENGTH);
            let object_offset = extent
                .object_offset
                .checked_add(consumed as u64)
                .ok_or(JournalStreamError::InvalidMapping)?;
            let logical_offset = extent
                .logical_offset
                .checked_add(consumed as u64)
                .ok_or(JournalStreamError::InvalidMapping)?;
            let range = self
                .reader
                .read_range(&locator, object_offset, chunk_length)?;
            validate_range_response(
                self.descriptor,
                &locator,
                object_offset,
                chunk_length,
                None,
                &range,
            )?;
            spans.push(CephFsJournalSourceSpan {
                locator: range.locator,
                logical_offset,
                object_offset,
                length: chunk_length as u64,
                range_sha256: format!("{:x}", Sha256::digest(&range.bytes)),
                provenance: range.provenance,
            });
            bytes.extend_from_slice(&range.bytes);
            consumed += chunk_length;
        }
        Ok(())
    }
}
