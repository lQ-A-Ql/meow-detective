use ceph_wire::CephFsFileLayout;

use super::CephFsFileDataReadError;
use crate::ceph_reconstruction::CephFsObjectReadProvenance;

pub const MAX_CEPHFS_INLINE_DATA_LENGTH: usize = 64 * 1024;
pub const CEPHFS_DATA_LOCATOR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFileDataDescriptor {
    pub filesystem_identity: String,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub inode: u64,
    pub file_size: u64,
    pub layout: CephFsFileLayout,
    pub inline_data: Option<Vec<u8>>,
}

impl CephFsFileDataDescriptor {
    pub fn new(
        filesystem_identity: impl Into<String>,
        filesystem_id: i64,
        fsmap_epoch: u32,
        inode: u64,
        file_size: u64,
        layout: CephFsFileLayout,
        inline_data: Option<Vec<u8>>,
    ) -> Result<Self, CephFsFileDataReadError> {
        let descriptor = Self {
            filesystem_identity: filesystem_identity.into(),
            filesystem_id,
            fsmap_epoch,
            inode,
            file_size,
            layout,
            inline_data,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub(super) fn validate(&self) -> Result<(), CephFsFileDataReadError> {
        if self.filesystem_identity.trim().is_empty()
            || self.filesystem_identity.contains('\0')
            || self.filesystem_id < 0
            || self.fsmap_epoch == 0
            || self.inode == 0
        {
            return Err(CephFsFileDataReadError::InvalidDescriptor(
                "filesystem identity, epoch, or inode is invalid",
            ));
        }
        self.layout
            .plan_range(self.file_size, 0, 0)
            .map_err(|_| CephFsFileDataReadError::InvalidLayout)?;
        match &self.inline_data {
            Some(bytes)
                if bytes.len() <= MAX_CEPHFS_INLINE_DATA_LENGTH
                    && u64::try_from(bytes.len()).ok() == Some(self.file_size) =>
            {
                Ok(())
            }
            Some(_) => Err(CephFsFileDataReadError::InvalidDescriptor(
                "inline bytes must exactly match file size and the CephFS inline limit",
            )),
            None if self.file_size == 0 || !self.layout.is_empty() => Ok(()),
            None => Err(CephFsFileDataReadError::InvalidDescriptor(
                "non-empty file has neither inline data nor an object layout",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CephFsDataObjectCacheKey {
    pub filesystem_identity: String,
    pub pool_id: i64,
    pub pool_namespace: String,
    pub object_name: String,
    pub fsmap_epoch: u32,
    pub locator_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDataObjectRead {
    pub cache_key: CephFsDataObjectCacheKey,
    pub locator: String,
    pub logical_offset: u64,
    pub object_offset: u64,
    pub length: usize,
    pub provenance: Vec<CephFsObjectReadProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFileDataRange {
    pub filesystem_identity: String,
    pub inode: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub object_reads: Vec<CephFsDataObjectRead>,
}
