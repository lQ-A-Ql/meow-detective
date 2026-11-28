use domain::DataSourceId;
use persistence_sqlite::repositories::processing_phase_repo::{
    DataSourceProcessingPhaseRecord, DataSourceProcessingPhaseRepo, ProcessingPhase,
    ProcessingPhaseCompletion, ProcessingPhaseTransition,
};

use super::catalog_publication::{StructurePublicationAttempt, StructurePublicationError};

pub struct CatalogPhaseAttempt {
    id: String,
    owner: String,
    version: u32,
}

impl CatalogPhaseAttempt {
    pub fn new(
        record: DataSourceProcessingPhaseRecord,
        version: u32,
    ) -> Result<Self, StructurePublicationError> {
        Ok(Self {
            id: record
                .attempt_id
                .ok_or(StructurePublicationError::InvalidPublication)?,
            owner: record
                .owner_id
                .ok_or(StructurePublicationError::InvalidPublication)?,
            version,
        })
    }
}

impl StructurePublicationAttempt for CatalogPhaseAttempt {
    fn attempt_id(&self) -> &str {
        &self.id
    }

    fn owner_id(&self) -> &str {
        &self.owner
    }

    fn refresh(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        DataSourceProcessingPhaseRepo::new(conn)
            .heartbeat(
                source,
                ProcessingPhase::Catalog,
                self.version,
                fingerprint,
                &self.owner,
                &self.id,
            )
            .map(|_| ())
            .map_err(StructurePublicationError::Database)
    }

    fn finish_ready(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        self.finish(
            conn,
            source,
            fingerprint,
            ProcessingPhaseTransition::ready("{}"),
        )
    }

    fn finish_failed(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
        error: &str,
        cancelled: bool,
    ) -> Result<(), StructurePublicationError> {
        let transition = if cancelled {
            ProcessingPhaseTransition::deferred("{}", Some(error))
        } else {
            ProcessingPhaseTransition::failed("{}", error)
        };
        self.finish(conn, source, fingerprint, transition)
    }
}

impl CatalogPhaseAttempt {
    fn finish(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
        transition: ProcessingPhaseTransition<'_>,
    ) -> Result<(), StructurePublicationError> {
        DataSourceProcessingPhaseRepo::new(conn)
            .finish(
                source,
                ProcessingPhase::Catalog,
                ProcessingPhaseCompletion::new(
                    self.version,
                    fingerprint,
                    &self.owner,
                    &self.id,
                    transition,
                ),
            )
            .map(|_| ())
            .map_err(StructurePublicationError::Database)
    }
}
