use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::source_runtime::content::SourceContent;

pub const ADAPTER_VERSION: &str = "fs-btrfs/pool-v1";
pub const POLICY_VERSION: &str = "btrfs-complete-committed-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BtrfsPoolMember {
    pub offset: u64,
    pub length: u64,
    pub identity: fs_btrfs::BtrfsDeviceIdentity,
    pub content: SourceContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BtrfsPoolPlan {
    pub adapter_version: String,
    pub policy_version: String,
    pub identity: fs_btrfs::BtrfsPoolIdentity,
    pub members: Vec<BtrfsPoolMember>,
}

impl BtrfsPoolPlan {
    pub fn validate(&self) -> Result<(), BtrfsSourceError> {
        if self.adapter_version != ADAPTER_VERSION
            || self.policy_version != POLICY_VERSION
            || self.members.len() < 2
            || self.members.len() > 64
            || self.members.len() != self.identity.devices.len()
        {
            return Err(BtrfsSourceError::Identity);
        }
        for (member, identity) in self.members.iter().zip(&self.identity.devices) {
            member.content.validate()?;
            if &member.identity != identity
                || member.identity.device_size > member.length
                || member
                    .offset
                    .checked_add(member.length)
                    .is_none_or(|end| end > member.content.logical_size)
            {
                return Err(BtrfsSourceError::Identity);
            }
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<String, BtrfsSourceError> {
        self.validate()?;
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BtrfsSourceError {
    #[error("Btrfs source publication failed: {0}")]
    Publication(
        #[from] persistence_source::publication::catalog_publication::StructurePublicationError,
    ),
    #[error("Btrfs source I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Btrfs source storage failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("Btrfs pool validation failed: {0}")]
    Pool(#[from] fs_btrfs::BtrfsPoolError),
    #[error("Btrfs source content changed: {0}")]
    Content(#[from] crate::source_runtime::content::ContentError),
    #[error("Btrfs descriptor is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Btrfs source identity or publication is inconsistent")]
    Identity,
    #[error("Btrfs source operation was cancelled")]
    Cancelled,
}

impl transport::ServiceErrorCategory for BtrfsSourceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Io(_) => transport::ErrorCategory::Io,
            _ => transport::ErrorCategory::Validation,
        }
    }
}
