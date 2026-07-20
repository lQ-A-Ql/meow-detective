use ceph_wire::CephFsFileLayout;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::CephFsFileDataReadError;
use crate::ceph_reconstruction::CephFsObjectReadProvenance;

pub const MAX_CEPHFS_INLINE_DATA_LENGTH: usize = 64 * 1024;
pub const CEPHFS_DATA_LOCATOR_VERSION: u32 = 1;
const CEPHFS_SPARSE_PROOF_DOMAIN: &[u8] = b"meow-detective/cephfs-sparse-hole/v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CephFsSparseExtentProof {
    pub offset: u64,
    pub length: u64,
    pub evidence_sha256: String,
    pub proof_sha256: String,
}

impl CephFsSparseExtentProof {
    pub fn new(
        inode: u64,
        offset: u64,
        length: u64,
        evidence_sha256: impl Into<String>,
        proof_sha256: impl Into<String>,
    ) -> Result<Self, CephFsFileDataReadError> {
        let proof = Self {
            offset,
            length,
            evidence_sha256: evidence_sha256.into(),
            proof_sha256: proof_sha256.into(),
        };
        proof.validate_for_inode(inode, u64::MAX)?;
        Ok(proof)
    }

    pub fn from_evidence(
        inode: u64,
        offset: u64,
        length: u64,
        evidence_sha256: impl Into<String>,
    ) -> Result<Self, CephFsFileDataReadError> {
        let evidence_sha256 = evidence_sha256.into();
        let proof_sha256 = sparse_extent_proof_sha256(inode, offset, length, &evidence_sha256);
        Self::new(inode, offset, length, evidence_sha256, proof_sha256)
    }

    pub fn validate_for_inode(
        &self,
        inode: u64,
        file_size: u64,
    ) -> Result<(), CephFsFileDataReadError> {
        if inode == 0 || self.length == 0 {
            return Err(CephFsFileDataReadError::InvalidSparseExtentProof(
                "sparse extent inode and length must be non-zero",
            ));
        }
        let end = self.offset.checked_add(self.length).ok_or(
            CephFsFileDataReadError::InvalidSparseExtentProof("sparse extent range overflows"),
        )?;
        if end > file_size {
            return Err(CephFsFileDataReadError::InvalidSparseExtentProof(
                "sparse extent exceeds file size",
            ));
        }
        if !is_sha256(&self.evidence_sha256)
            || !is_sha256(&self.proof_sha256)
            || self.proof_sha256
                != sparse_extent_proof_sha256(
                    inode,
                    self.offset,
                    self.length,
                    &self.evidence_sha256,
                )
        {
            return Err(CephFsFileDataReadError::InvalidSparseExtentProof(
                "sparse extent proof does not bind to inode and range",
            ));
        }
        Ok(())
    }

    pub fn end(&self) -> u64 {
        self.offset.saturating_add(self.length)
    }
}

pub fn sparse_extent_proof_sha256(
    inode: u64,
    offset: u64,
    length: u64,
    evidence_sha256: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(CEPHFS_SPARSE_PROOF_DOMAIN);
    digest.update(inode.to_le_bytes());
    digest.update(offset.to_le_bytes());
    digest.update(length.to_le_bytes());
    digest.update((evidence_sha256.len() as u64).to_le_bytes());
    digest.update(evidence_sha256.as_bytes());
    hex::encode(digest.finalize())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFileDataDescriptor {
    pub filesystem_identity: String,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub inode: u64,
    pub file_size: u64,
    pub layout: CephFsFileLayout,
    pub inline_data: Option<Vec<u8>>,
    pub sparse_extents: Vec<CephFsSparseExtentProof>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFileDataContent {
    pub inline_data: Option<Vec<u8>>,
    pub sparse_extents: Vec<CephFsSparseExtentProof>,
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
        Self::with_content(
            filesystem_identity,
            filesystem_id,
            fsmap_epoch,
            inode,
            file_size,
            layout,
            CephFsFileDataContent {
                inline_data,
                sparse_extents: Vec::new(),
            },
        )
    }

    pub fn with_sparse_extents(
        filesystem_identity: impl Into<String>,
        filesystem_id: i64,
        fsmap_epoch: u32,
        inode: u64,
        file_size: u64,
        layout: CephFsFileLayout,
        sparse_extents: Vec<CephFsSparseExtentProof>,
    ) -> Result<Self, CephFsFileDataReadError> {
        Self::with_content(
            filesystem_identity,
            filesystem_id,
            fsmap_epoch,
            inode,
            file_size,
            layout,
            CephFsFileDataContent {
                inline_data: None,
                sparse_extents,
            },
        )
    }

    pub fn with_content(
        filesystem_identity: impl Into<String>,
        filesystem_id: i64,
        fsmap_epoch: u32,
        inode: u64,
        file_size: u64,
        layout: CephFsFileLayout,
        content: CephFsFileDataContent,
    ) -> Result<Self, CephFsFileDataReadError> {
        let descriptor = Self {
            filesystem_identity: filesystem_identity.into(),
            filesystem_id,
            fsmap_epoch,
            inode,
            file_size,
            layout,
            inline_data: content.inline_data,
            sparse_extents: content.sparse_extents,
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
        validate_sparse_extents(self)?;
        match &self.inline_data {
            Some(bytes)
                if bytes.len() <= MAX_CEPHFS_INLINE_DATA_LENGTH
                    && u64::try_from(bytes.len()).ok() == Some(self.file_size)
                    && self.sparse_extents.is_empty() =>
            {
                Ok(())
            }
            Some(_) => Err(CephFsFileDataReadError::InvalidDescriptor(
                "inline bytes must exactly match file size, the inline limit, and have no sparse extents",
            )),
            None if self.file_size == 0 || !self.layout.is_empty() => Ok(()),
            None if covers_range(&self.sparse_extents, 0, self.file_size) => Ok(()),
            None => Err(CephFsFileDataReadError::InvalidDescriptor(
                "non-empty file has neither inline data, sparse proof, nor an object layout",
            )),
        }
    }
}

pub(crate) fn validate_sparse_extents(
    descriptor: &CephFsFileDataDescriptor,
) -> Result<(), CephFsFileDataReadError> {
    let mut previous_end = 0;
    for (index, extent) in descriptor.sparse_extents.iter().enumerate() {
        extent.validate_for_inode(descriptor.inode, descriptor.file_size)?;
        if index > 0 && extent.offset < previous_end {
            return Err(CephFsFileDataReadError::InvalidSparseExtentProof(
                "sparse extents overlap or are not ordered",
            ));
        }
        previous_end = extent.end();
    }
    Ok(())
}

pub(crate) fn covers_range(extents: &[CephFsSparseExtentProof], offset: u64, end: u64) -> bool {
    if offset >= end {
        return true;
    }
    let mut cursor = offset;
    for extent in extents {
        if extent.offset > cursor {
            return false;
        }
        if extent.end() > cursor {
            cursor = extent.end();
        }
        if cursor >= end {
            return true;
        }
    }
    false
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
