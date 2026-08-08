mod catalog;
mod digest;
mod query;
mod records;
mod validation;
mod write;

use rusqlite::Connection;
use thiserror::Error;

use crate::connection::DbError;

pub use digest::{cephfs_namespace_projection_digest, cephfs_namespace_projection_sha256};
pub use records::{
    CephFsDentryRecord, CephFsFileCatalogSummary, CephFsFileLayoutRecord, CephFsFileLocatorRecord,
    CephFsInodeRecord, CephFsNamespaceDiagnosticRecord, CephFsNamespaceManifest,
    CephFsNamespaceProjection, CephFsNamespaceWriteOutcome, CephFsPublishedCatalog,
    CephFsSparseExtentRecord, CEPHFS_NAMESPACE_DECODER_PROFILE, CEPHFS_NAMESPACE_SCHEMA_VERSION,
};

#[derive(Debug, Error)]
pub enum CephFsNamespaceRepoError {
    #[error("invalid CephFS namespace projection: {0}")]
    Invalid(&'static str),
    #[error("CephFS namespace projection is non-deterministic for the same input")]
    DeterminismConflict,
    #[error(transparent)]
    Database(#[from] DbError),
}

impl From<rusqlite::Error> for CephFsNamespaceRepoError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(DbError::from(error))
    }
}

pub type CephFsNamespaceRepoResult<T> = Result<T, CephFsNamespaceRepoError>;

pub struct CephFsNamespaceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephFsNamespaceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find(
        &self,
        filesystem_identity: &str,
        data_source_id: &str,
    ) -> CephFsNamespaceRepoResult<Option<CephFsNamespaceProjection>> {
        let projection = query::find(self.conn, filesystem_identity, data_source_id)?;
        if let Some(projection) = &projection {
            validation::validate_projection(projection)?;
        }
        Ok(projection)
    }

    pub fn find_manifest(
        &self,
        filesystem_identity: &str,
        data_source_id: &str,
    ) -> CephFsNamespaceRepoResult<Option<CephFsNamespaceManifest>> {
        query::find_manifest(self.conn, filesystem_identity, data_source_id)
    }

    /// Validate the complete published projection before exposing it as a
    /// browseable namespace.  A SQLite quick check only validates page
    /// structure; this path also verifies cross-row invariants and the
    /// canonical projection digest.
    pub fn verify_published(
        &self,
        filesystem_identity: &str,
        data_source_id: &str,
    ) -> CephFsNamespaceRepoResult<Option<CephFsNamespaceManifest>> {
        let Some(projection) = self.find(filesystem_identity, data_source_id)? else {
            return Ok(None);
        };
        if !projection.manifest.published || projection.manifest.completeness != "closed" {
            return Err(CephFsNamespaceRepoError::Invalid(
                "published namespace is not closed",
            ));
        }
        Ok(Some(projection.manifest))
    }

    pub fn verify_published_catalog(
        &self,
        filesystem_identity: &str,
        data_source_id: &str,
        expected_root_name: &str,
    ) -> CephFsNamespaceRepoResult<CephFsPublishedCatalog> {
        let manifest = self
            .verify_published(filesystem_identity, data_source_id)?
            .ok_or(CephFsNamespaceRepoError::Invalid(
                "published namespace manifest is missing",
            ))?;
        let summary = catalog::verify_published_catalog(self.conn, &manifest, expected_root_name)?;
        Ok(CephFsPublishedCatalog { manifest, summary })
    }

    pub fn find_file_locator(
        &self,
        data_source_id: &str,
        entry_id: &str,
    ) -> CephFsNamespaceRepoResult<Option<CephFsFileLocatorRecord>> {
        query::find_file_locator(self.conn, data_source_id, entry_id)
    }

    pub fn replace(
        &self,
        projection: &CephFsNamespaceProjection,
    ) -> CephFsNamespaceRepoResult<CephFsNamespaceWriteOutcome> {
        validation::validate_projection(projection)?;
        write::replace(self.conn, projection)
    }
}
