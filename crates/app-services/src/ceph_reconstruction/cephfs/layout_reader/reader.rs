use ceph_wire::{format_cephfs_data_object_name, CephFsLayoutSegment};

use super::{
    CephFsDataObjectCacheKey, CephFsDataObjectRead, CephFsFileDataDescriptor, CephFsFileDataRange,
    CephFsFileDataReadError, CEPHFS_DATA_LOCATOR_VERSION,
};
use crate::ceph_reconstruction::{
    CephFsObjectLocator, CephFsObjectRange, CephFsObjectRangeReader, MAX_CEPHFS_OBJECT_RANGE_LENGTH,
};

pub struct CephFsDataRangeReader<R> {
    descriptor: CephFsFileDataDescriptor,
    object_reader: R,
}

impl<R> CephFsDataRangeReader<R>
where
    R: CephFsObjectRangeReader,
{
    pub fn new(
        descriptor: CephFsFileDataDescriptor,
        object_reader: R,
    ) -> Result<Self, CephFsFileDataReadError> {
        descriptor.validate()?;
        Ok(Self {
            descriptor,
            object_reader,
        })
    }

    pub fn descriptor(&self) -> &CephFsFileDataDescriptor {
        &self.descriptor
    }

    pub fn read_range(
        &mut self,
        offset: u64,
        length: usize,
    ) -> Result<CephFsFileDataRange, CephFsFileDataReadError> {
        let end = validate_range(&self.descriptor, offset, length)?;
        if let Some(inline_data) = &self.descriptor.inline_data {
            let start =
                usize::try_from(offset).map_err(|_| CephFsFileDataReadError::RangeOverflow)?;
            let end = usize::try_from(end).map_err(|_| CephFsFileDataReadError::RangeOverflow)?;
            return Ok(CephFsFileDataRange {
                filesystem_identity: self.descriptor.filesystem_identity.clone(),
                inode: self.descriptor.inode,
                offset,
                bytes: inline_data[start..end].to_vec(),
                object_reads: Vec::new(),
            });
        }
        self.read_object_range(offset, length)
    }

    pub fn into_inner(self) -> R {
        self.object_reader
    }

    fn read_object_range(
        &mut self,
        offset: u64,
        length: usize,
    ) -> Result<CephFsFileDataRange, CephFsFileDataReadError> {
        let segments = self
            .descriptor
            .layout
            .plan_range(self.descriptor.file_size, offset, length)
            .map_err(|_| CephFsFileDataReadError::InvalidLayout)?;
        let mut bytes = Vec::with_capacity(length);
        let mut object_reads = Vec::with_capacity(segments.len());
        for segment in segments {
            let (range, cache_key) = self.read_segment(&segment)?;
            bytes.extend_from_slice(&range.bytes);
            object_reads.push(CephFsDataObjectRead {
                cache_key,
                locator: range.locator,
                logical_offset: segment.logical_offset,
                object_offset: segment.object_offset,
                length: range.bytes.len(),
                provenance: range.provenance,
            });
        }
        if bytes.len() != length {
            return Err(CephFsFileDataReadError::ResponseMismatch {
                locator: format!("inode:{:x}", self.descriptor.inode),
            });
        }
        Ok(CephFsFileDataRange {
            filesystem_identity: self.descriptor.filesystem_identity.clone(),
            inode: self.descriptor.inode,
            offset,
            bytes,
            object_reads,
        })
    }

    fn read_segment(
        &mut self,
        segment: &CephFsLayoutSegment,
    ) -> Result<(CephFsObjectRange, CephFsDataObjectCacheKey), CephFsFileDataReadError> {
        let object_name =
            format_cephfs_data_object_name(self.descriptor.inode, segment.object_number)
                .map_err(|_| CephFsFileDataReadError::InvalidLocator)?;
        let locator = CephFsObjectLocator::new(
            self.descriptor.filesystem_id,
            self.descriptor.layout.pool_id,
            self.descriptor.layout.pool_namespace.as_bytes().to_vec(),
            object_name.as_bytes().to_vec(),
            self.descriptor.fsmap_epoch,
        )
        .map_err(|_| CephFsFileDataReadError::InvalidLocator)?;
        let length =
            usize::try_from(segment.length).map_err(|_| CephFsFileDataReadError::RangeOverflow)?;
        let range = self
            .object_reader
            .read_range(&locator, segment.object_offset, length)?;
        validate_response(&self.descriptor, &locator, segment, &range)?;
        let cache_key = CephFsDataObjectCacheKey {
            filesystem_identity: self.descriptor.filesystem_identity.clone(),
            pool_id: self.descriptor.layout.pool_id,
            pool_namespace: self.descriptor.layout.pool_namespace.clone(),
            object_name,
            fsmap_epoch: self.descriptor.fsmap_epoch,
            locator_version: CEPHFS_DATA_LOCATOR_VERSION,
        };
        Ok((range, cache_key))
    }
}

fn validate_range(
    descriptor: &CephFsFileDataDescriptor,
    offset: u64,
    length: usize,
) -> Result<u64, CephFsFileDataReadError> {
    if length > MAX_CEPHFS_OBJECT_RANGE_LENGTH {
        return Err(CephFsFileDataReadError::RangeTooLarge {
            requested: length,
            maximum: MAX_CEPHFS_OBJECT_RANGE_LENGTH,
        });
    }
    let length = u64::try_from(length).map_err(|_| CephFsFileDataReadError::RangeOverflow)?;
    let end = offset
        .checked_add(length)
        .ok_or(CephFsFileDataReadError::RangeOverflow)?;
    if offset > descriptor.file_size || end > descriptor.file_size {
        return Err(CephFsFileDataReadError::RangeOutOfBounds {
            offset,
            length,
            file_size: descriptor.file_size,
        });
    }
    Ok(end)
}

fn validate_response(
    descriptor: &CephFsFileDataDescriptor,
    locator: &CephFsObjectLocator,
    segment: &CephFsLayoutSegment,
    range: &CephFsObjectRange,
) -> Result<(), CephFsFileDataReadError> {
    let length = usize::try_from(segment.length).ok();
    let end = length
        .and_then(|length| u64::try_from(length).ok())
        .and_then(|length| segment.object_offset.checked_add(length));
    if range.filesystem_identity != descriptor.filesystem_identity
        || range.locator != locator.canonical()
        || range.offset != segment.object_offset
        || Some(range.bytes.len()) != length
        || range.provenance.is_empty()
        || end.is_none_or(|end| end > range.object_size)
    {
        return Err(CephFsFileDataReadError::ResponseMismatch {
            locator: locator.canonical(),
        });
    }
    Ok(())
}
