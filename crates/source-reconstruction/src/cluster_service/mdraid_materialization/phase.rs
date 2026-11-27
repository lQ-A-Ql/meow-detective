use domain::DataSourceId;
use evidence_access::mdraid::MdRaidPlanError;
use persistence_sqlite::repositories::processing_phase_repo::{
    DataSourceProcessingPhaseRecord, DataSourceProcessingPhaseRepo, ProcessingPhase,
    ProcessingPhaseCompletion, ProcessingPhaseTransition,
};

use persistence_source::publication::catalog_publication::{
    StructurePublicationAttempt, StructurePublicationError,
};

pub(super) const VERSION: u32 = 2;

pub(super) struct CatalogAttempt {
    pub id: String,
    owner: String,
}

impl CatalogAttempt {
    pub(super) fn new(record: DataSourceProcessingPhaseRecord) -> Result<Self, MdRaidPlanError> {
        Ok(Self {
            id: record
                .attempt_id
                .ok_or(MdRaidPlanError::InvalidPublication)?,
            owner: record.owner_id.ok_or(MdRaidPlanError::InvalidPublication)?,
        })
    }

    pub(super) fn refresh(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
    ) -> Result<(), MdRaidPlanError> {
        DataSourceProcessingPhaseRepo::new(conn).heartbeat(
            source,
            ProcessingPhase::Catalog,
            VERSION,
            fingerprint,
            &self.owner,
            &self.id,
        )?;
        Ok(())
    }

    pub(super) fn finish_ready(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
    ) -> Result<(), MdRaidPlanError> {
        self.finish(
            conn,
            source,
            fingerprint,
            ProcessingPhaseTransition::ready("{}"),
        )
    }

    pub(super) fn finish_failed(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
        error: &str,
        cancelled: bool,
    ) -> Result<(), MdRaidPlanError> {
        let transition = if cancelled {
            ProcessingPhaseTransition::deferred("{}", Some(error))
        } else {
            ProcessingPhaseTransition::failed("{}", error)
        };
        self.finish(conn, source, fingerprint, transition)
    }

    fn finish(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
        transition: ProcessingPhaseTransition<'_>,
    ) -> Result<(), MdRaidPlanError> {
        DataSourceProcessingPhaseRepo::new(conn).finish(
            source,
            ProcessingPhase::Catalog,
            ProcessingPhaseCompletion::new(VERSION, fingerprint, &self.owner, &self.id, transition),
        )?;
        Ok(())
    }
}

impl StructurePublicationAttempt for CatalogAttempt {
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
        CatalogAttempt::refresh(self, conn, source, fingerprint).map_err(map_error)
    }

    fn finish_ready(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        CatalogAttempt::finish_ready(self, conn, source, fingerprint).map_err(map_error)
    }

    fn finish_failed(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
        error: &str,
        cancelled: bool,
    ) -> Result<(), StructurePublicationError> {
        CatalogAttempt::finish_failed(self, conn, source, fingerprint, error, cancelled)
            .map_err(map_error)
    }
}

fn map_error(error: MdRaidPlanError) -> StructurePublicationError {
    match error {
        MdRaidPlanError::Database(error) => StructurePublicationError::Database(error),
        MdRaidPlanError::Cancelled => StructurePublicationError::Cancelled,
        other => StructurePublicationError::Validation(other.to_string()),
    }
}
