mod digest;
mod query;
mod records;
mod validation;
mod write;

use rusqlite::Connection;
use thiserror::Error;

use crate::connection::DbError;

pub use digest::cephfs_metadata_inventory_sha256;
pub use records::{
    CephFsMetadataInventory, CephFsMetadataInventoryManifest, CephFsMetadataObjectProjection,
    CephFsMetadataWriteOutcome, CEPHFS_METADATA_CLASSIFIER_PROFILE, CEPHFS_METADATA_SCHEMA_VERSION,
};

#[derive(Debug, Error)]
pub enum CephFsMetadataInventoryRepoError {
    #[error("invalid CephFS metadata inventory: {0}")]
    Invalid(&'static str),
    #[error("CephFS metadata inventory is non-deterministic for the same source snapshot")]
    DeterminismConflict,
    #[error("CephFS metadata inventory crosses the bound metadata pool")]
    CrossPoolReference,
    #[error(transparent)]
    Database(#[from] DbError),
}

pub type CephFsMetadataInventoryRepoResult<T> = Result<T, CephFsMetadataInventoryRepoError>;

pub struct CephFsMetadataInventoryRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephFsMetadataInventoryRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find(
        &self,
        filesystem_identity: &str,
        inventory_id: &str,
    ) -> CephFsMetadataInventoryRepoResult<Option<CephFsMetadataInventory>> {
        query::find(self.conn, filesystem_identity, inventory_id)
    }

    pub fn find_manifest(
        &self,
        filesystem_identity: &str,
        inventory_id: &str,
    ) -> CephFsMetadataInventoryRepoResult<Option<CephFsMetadataInventoryManifest>> {
        query::find_manifest(self.conn, filesystem_identity, inventory_id)
    }

    pub fn find_object_by_locator(
        &self,
        filesystem_identity: &str,
        inventory_id: &str,
        locator: &str,
    ) -> CephFsMetadataInventoryRepoResult<Option<CephFsMetadataObjectProjection>> {
        query::find_object_by_locator(self.conn, filesystem_identity, inventory_id, locator)
    }

    pub fn replace(
        &self,
        inventory: &CephFsMetadataInventory,
    ) -> CephFsMetadataInventoryRepoResult<CephFsMetadataWriteOutcome> {
        validate_cephfs_metadata_inventory(inventory)?;
        write::replace(self.conn, inventory)
    }
}

pub fn validate_cephfs_metadata_inventory(
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<()> {
    validation::validate_inventory(inventory)
}
