mod digest;
mod event_validation;
mod query;
mod records;
mod validation;
mod write;

use rusqlite::Connection;
use thiserror::Error;

use crate::connection::DbError;

pub use digest::{
    cephfs_journal_input_sha256, cephfs_journal_map_provenance_sha256,
    cephfs_journal_projection_sha256,
};
pub use records::{
    cephfs_journal_u64_hex, CephFsJournalEventRecord, CephFsJournalEventSpanRecord,
    CephFsJournalMapProvenanceRecord, CephFsJournalReplayManifest, CephFsJournalReplayProjection,
    CephFsJournalWriteOutcome, CEPHFS_JOURNAL_DECODER_PROFILE, CEPHFS_JOURNAL_SCHEMA_VERSION,
};

#[derive(Debug, Error)]
pub enum CephFsJournalRepoError {
    #[error("invalid CephFS journal projection: {0}")]
    Invalid(&'static str),
    #[error("CephFS journal projection is not bound to its metadata inventory")]
    SourceBindingMismatch,
    #[error("CephFS journal control or event span references an unknown metadata object")]
    ObjectBindingMismatch,
    #[error("CephFS journal replay is non-deterministic for the same input")]
    DeterminismConflict,
    #[error(transparent)]
    Database(#[from] DbError),
}

pub type CephFsJournalRepoResult<T> = Result<T, CephFsJournalRepoError>;

pub struct CephFsJournalRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephFsJournalRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find(
        &self,
        filesystem_identity: &str,
        inventory_id: &str,
        rank: u32,
    ) -> CephFsJournalRepoResult<Option<CephFsJournalReplayProjection>> {
        let projection = query::find(self.conn, filesystem_identity, inventory_id, rank)?;
        if let Some(projection) = &projection {
            validation::validate_projection(projection)?;
        }
        Ok(projection)
    }

    pub fn replace(
        &self,
        projection: &CephFsJournalReplayProjection,
    ) -> CephFsJournalRepoResult<CephFsJournalWriteOutcome> {
        validation::validate_projection(projection)?;
        write::replace(self.conn, projection)
    }
}

pub fn validate_cephfs_journal_projection(
    projection: &CephFsJournalReplayProjection,
) -> CephFsJournalRepoResult<()> {
    validation::validate_projection(projection)
}
