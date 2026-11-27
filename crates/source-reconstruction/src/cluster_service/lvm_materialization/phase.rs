use domain::DataSourceId;
use evidence_access::lvm::LvmPlanError;
use persistence_sqlite::repositories::processing_phase_repo::{
    DataSourceProcessingPhaseRecord, DataSourceProcessingPhaseRepo, ProcessingPhase,
    ProcessingPhaseCompletion, ProcessingPhaseTransition,
};

use persistence_source::publication::catalog_publication::{
    StructurePublicationAttempt, StructurePublicationError,
};

pub(super) const VERSION: u32 = 1;

pub(super) struct LvmAttempt {
    id: String,
    owner: String,
}

impl LvmAttempt {
    pub(super) fn new(record: DataSourceProcessingPhaseRecord) -> Result<Self, LvmPlanError> {
        Ok(Self {
            id: record.attempt_id.ok_or(LvmPlanError::InvalidPublication)?,
            owner: record.owner_id.ok_or(LvmPlanError::InvalidPublication)?,
        })
    }
}

impl StructurePublicationAttempt for LvmAttempt {
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
                VERSION,
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

impl LvmAttempt {
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
                    VERSION,
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
