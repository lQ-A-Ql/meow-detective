use domain::DataSourceId;
use evidence_access::mdraid::MdRaidPlanError;
use persistence_sqlite::repositories::processing_phase_repo::{
    DataSourceProcessingPhaseRecord, DataSourceProcessingPhaseRepo, ProcessingPhase,
    ProcessingPhaseCompletion, ProcessingPhaseTransition,
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
