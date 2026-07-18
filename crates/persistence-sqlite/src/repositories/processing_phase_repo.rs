use domain::DataSourceId;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::connection::{DbError, DbResult};

mod storage;
mod types;
mod validation;

use storage::{decode_record, find_on, read_stored, update_transition};
use validation::{
    ensure_derived_source, invalid, transition_matches, valid_text, validate_attempt,
    validate_data_source_id, validate_identity, validate_transition_identity,
    validate_transition_payload,
};

pub use types::{
    DataSourceProcessingPhaseRecord, ProcessingPhase, ProcessingPhaseClaim,
    ProcessingPhaseCompletion, ProcessingPhaseState, ProcessingPhaseTransition,
};

const PROCESSING_PHASE_LEASE_SECONDS: i64 = 4 * 60 * 60;

pub struct DataSourceProcessingPhaseRepo<'a> {
    conn: &'a Connection,
}

impl<'a> DataSourceProcessingPhaseRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Inserts pending work, or resets an existing row only when its identity changes.
    pub fn upsert(
        &self,
        data_source_id: &DataSourceId,
        phase: ProcessingPhase,
        version: u32,
        input_fingerprint: &str,
    ) -> DbResult<DataSourceProcessingPhaseRecord> {
        validate_identity(data_source_id, version, input_fingerprint)?;
        ensure_derived_source(self.conn, data_source_id)?;
        if let Some(current) = self.find(data_source_id, phase)? {
            let identity_changed =
                current.version != version || current.input_fingerprint != input_fingerprint;
            if identity_changed && current.state == ProcessingPhaseState::Running {
                let active = self.conn.query_row(
                    "SELECT EXISTS (
                         SELECT 1
                         FROM data_source_processing_phases
                         WHERE data_source_id = ?1
                           AND phase = ?2
                           AND state = 'running'
                           AND lease_expires_at > datetime('now')
                     )",
                    params![data_source_id.0, phase.as_str()],
                    |row| row.get::<_, i64>(0),
                )? == 1;
                if active {
                    return invalid(
                        "processing phase identity cannot change while its lease is active",
                    );
                }
            }
        }
        self.conn.execute(
            "INSERT INTO data_source_processing_phases (
                data_source_id, phase, state, version, input_fingerprint, stats_json
             ) VALUES (?1, ?2, 'pending', ?3, ?4, '{}')
             ON CONFLICT(data_source_id, phase) DO UPDATE SET
                 state = 'pending',
                 version = excluded.version,
                 input_fingerprint = excluded.input_fingerprint,
                 owner_id = NULL,
                 attempt_id = NULL,
                 stats_json = '{}',
                 last_error = NULL,
                 started_at = NULL,
                 completed_at = NULL,
                 heartbeat_at = NULL,
                 lease_expires_at = NULL,
                 updated_at = datetime('now')
             WHERE data_source_processing_phases.version <> excluded.version
                OR data_source_processing_phases.input_fingerprint
                   <> excluded.input_fingerprint",
            params![data_source_id.0, phase.as_str(), version, input_fingerprint],
        )?;
        self.find(data_source_id, phase)?.ok_or_else(|| {
            DbError::System("processing phase upsert did not produce a row".to_string())
        })
    }

    pub fn claim(
        &self,
        data_source_id: &DataSourceId,
        phase: ProcessingPhase,
        version: u32,
        input_fingerprint: &str,
        owner_id: &str,
    ) -> DbResult<ProcessingPhaseClaim> {
        validate_identity(data_source_id, version, input_fingerprint)?;
        if !valid_text(owner_id) {
            return invalid("processing phase owner ID is invalid");
        }
        let transaction = self.conn.unchecked_transaction()?;
        let claim = {
            let repo = DataSourceProcessingPhaseRepo { conn: &transaction };
            repo.claim_in_transaction(data_source_id, phase, version, input_fingerprint, owner_id)?
        };
        transaction.commit()?;
        Ok(claim)
    }

    fn claim_in_transaction(
        &self,
        data_source_id: &DataSourceId,
        phase: ProcessingPhase,
        version: u32,
        input_fingerprint: &str,
        owner_id: &str,
    ) -> DbResult<ProcessingPhaseClaim> {
        let current = self.upsert(data_source_id, phase, version, input_fingerprint)?;
        if current.state == ProcessingPhaseState::Ready {
            return Ok(ProcessingPhaseClaim::Ready(current));
        }
        let attempt_id = Uuid::new_v4().to_string();
        let lease_modifier = format!("+{PROCESSING_PHASE_LEASE_SECONDS} seconds");
        let affected = self.conn.execute(
            "UPDATE data_source_processing_phases
             SET state = 'running',
                 owner_id = ?1,
                 attempt_id = ?2,
                 stats_json = '{}',
                 last_error = NULL,
                 started_at = datetime('now'),
                 completed_at = NULL,
                 heartbeat_at = datetime('now'),
                 lease_expires_at = datetime('now', ?7),
                 updated_at = datetime('now')
             WHERE data_source_id = ?3
               AND phase = ?4
               AND version = ?5
               AND input_fingerprint = ?6
               AND (
                    state IN ('pending', 'failed', 'deferred')
                    OR (
                        state = 'running'
                        AND lease_expires_at <= datetime('now')
                    )
               )",
            params![
                owner_id,
                attempt_id,
                data_source_id.0,
                phase.as_str(),
                version,
                input_fingerprint,
                lease_modifier,
            ],
        )?;
        let record = self.find(data_source_id, phase)?.ok_or_else(|| {
            DbError::System("processing phase disappeared while it was claimed".to_string())
        })?;
        validate_transition_identity(&record, version, input_fingerprint)?;
        if affected == 1 {
            return Ok(ProcessingPhaseClaim::Acquired(record));
        }
        match record.state {
            ProcessingPhaseState::Ready => Ok(ProcessingPhaseClaim::Ready(record)),
            ProcessingPhaseState::Running => Ok(ProcessingPhaseClaim::Busy(record)),
            _ => invalid("processing phase claim lost an unexpected state transition"),
        }
    }

    /// Marks only expired running phases as failed.
    ///
    /// An unexpired lease may still belong to a live worker, so recovery must
    /// not invalidate it merely because another application opened the case.
    /// Failed phases remain retryable through the normal claim path.
    pub fn recover_interrupted(&self, reason: &str) -> DbResult<usize> {
        if !valid_text(reason) {
            return invalid("processing phase recovery reason is invalid");
        }
        self.conn
            .execute(
                "UPDATE data_source_processing_phases
                 SET state = 'failed',
                     last_error = ?1,
                     completed_at = datetime('now'),
                     heartbeat_at = datetime('now'),
                     lease_expires_at = NULL,
                     updated_at = datetime('now')
                 WHERE state = 'running'
                   AND lease_expires_at <= datetime('now')",
                [reason],
            )
            .map_err(Into::into)
    }

    pub fn finish(
        &self,
        data_source_id: &DataSourceId,
        phase: ProcessingPhase,
        completion: ProcessingPhaseCompletion<'_>,
    ) -> DbResult<DataSourceProcessingPhaseRecord> {
        validate_identity(
            data_source_id,
            completion.version,
            completion.input_fingerprint,
        )?;
        validate_attempt(completion.owner_id, completion.attempt_id)?;
        validate_transition_payload(completion.transition)?;
        let affected = update_transition(self.conn, data_source_id, phase, completion)?;
        let record = self.find(data_source_id, phase)?.ok_or_else(|| {
            DbError::System("processing phase disappeared while it was completed".to_string())
        })?;
        validate_transition_identity(&record, completion.version, completion.input_fingerprint)?;
        if affected == 1
            || transition_matches(
                &record,
                completion.owner_id,
                completion.attempt_id,
                completion.transition,
            )
        {
            return Ok(record);
        }
        invalid("processing phase completion belongs to a stale or inactive attempt")
    }

    pub fn heartbeat(
        &self,
        data_source_id: &DataSourceId,
        phase: ProcessingPhase,
        version: u32,
        input_fingerprint: &str,
        owner_id: &str,
        attempt_id: &str,
    ) -> DbResult<DataSourceProcessingPhaseRecord> {
        validate_identity(data_source_id, version, input_fingerprint)?;
        validate_attempt(owner_id, attempt_id)?;
        let lease_modifier = format!("+{PROCESSING_PHASE_LEASE_SECONDS} seconds");
        let affected = self.conn.execute(
            "UPDATE data_source_processing_phases
             SET heartbeat_at = datetime('now'),
                 lease_expires_at = datetime('now', ?7),
                 updated_at = datetime('now')
             WHERE data_source_id = ?1
               AND phase = ?2
               AND version = ?3
               AND input_fingerprint = ?4
               AND state = 'running'
               AND owner_id = ?5
               AND attempt_id = ?6",
            params![
                data_source_id.0,
                phase.as_str(),
                version,
                input_fingerprint,
                owner_id,
                attempt_id,
                lease_modifier,
            ],
        )?;
        if affected != 1 {
            return invalid("processing phase heartbeat belongs to a stale or inactive attempt");
        }
        self.find(data_source_id, phase)?.ok_or_else(|| {
            DbError::System("processing phase disappeared after its heartbeat".to_string())
        })
    }

    pub fn find(
        &self,
        data_source_id: &DataSourceId,
        phase: ProcessingPhase,
    ) -> DbResult<Option<DataSourceProcessingPhaseRecord>> {
        validate_data_source_id(data_source_id)?;
        find_on(self.conn, data_source_id, phase)
    }

    pub fn list_for_data_source(
        &self,
        data_source_id: &DataSourceId,
    ) -> DbResult<Vec<DataSourceProcessingPhaseRecord>> {
        validate_data_source_id(data_source_id)?;
        let mut statement = self.conn.prepare(
            "SELECT data_source_id, phase, state, version, input_fingerprint,
                    owner_id, attempt_id, stats_json, last_error, started_at,
                    completed_at, heartbeat_at, lease_expires_at, updated_at
             FROM data_source_processing_phases
             WHERE data_source_id = ?1
             ORDER BY CASE phase
                WHEN 'catalog' THEN 0
                WHEN 'graph' THEN 1
                WHEN 'platform' THEN 2
                WHEN 'artifacts' THEN 3
                WHEN 'timeline' THEN 4
                WHEN 'search' THEN 5
             END",
        )?;
        let stored = statement
            .query_map([&data_source_id.0], read_stored)?
            .collect::<Result<Vec<_>, _>>()?;
        stored.into_iter().map(decode_record).collect()
    }
}
