use domain::DataSourceId;
use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

use super::{
    types::{
        DataSourceProcessingPhaseRecord, ProcessingPhase, ProcessingPhaseCompletion,
        ProcessingPhaseState,
    },
    validation::{invalid, validate_record},
};

pub(super) fn find_on(
    conn: &Connection,
    data_source_id: &DataSourceId,
    phase: ProcessingPhase,
) -> DbResult<Option<DataSourceProcessingPhaseRecord>> {
    let stored = conn
        .query_row(
            "SELECT data_source_id, phase, state, version, input_fingerprint,
                    owner_id, attempt_id, stats_json, last_error, started_at,
                    completed_at, heartbeat_at, lease_expires_at, updated_at
             FROM data_source_processing_phases
             WHERE data_source_id = ?1 AND phase = ?2",
            params![data_source_id.0, phase.as_str()],
            read_stored,
        )
        .optional()?;
    stored.map(decode_record).transpose()
}

pub(super) fn update_transition(
    conn: &Connection,
    data_source_id: &DataSourceId,
    phase: ProcessingPhase,
    completion: ProcessingPhaseCompletion<'_>,
) -> DbResult<usize> {
    let sql = match completion.transition.state {
        ProcessingPhaseState::Ready => terminal_update_sql("ready"),
        ProcessingPhaseState::Failed => terminal_update_sql("failed"),
        ProcessingPhaseState::Deferred => terminal_update_sql("deferred"),
        ProcessingPhaseState::Pending | ProcessingPhaseState::Running => {
            return invalid("processing phase completion requires a terminal state")
        }
    };
    conn.execute(
        &sql,
        params![
            completion.transition.stats_json,
            data_source_id.0,
            phase.as_str(),
            completion.version,
            completion.input_fingerprint,
            completion.owner_id,
            completion.attempt_id,
            completion.transition.last_error
        ],
    )
    .map_err(Into::into)
}

fn terminal_update_sql(state: &str) -> String {
    format!(
        "UPDATE data_source_processing_phases
         SET state = '{state}', stats_json = ?1, last_error = ?8,
             completed_at = datetime('now'),
             heartbeat_at = datetime('now'),
             lease_expires_at = NULL,
             updated_at = datetime('now')
         WHERE data_source_id = ?2
           AND phase = ?3
           AND version = ?4
           AND input_fingerprint = ?5
           AND state = 'running'
           AND owner_id = ?6
           AND attempt_id = ?7"
    )
}

pub(super) struct StoredProcessingPhaseRecord {
    data_source_id: String,
    phase: String,
    state: String,
    version: i64,
    input_fingerprint: String,
    owner_id: Option<String>,
    attempt_id: Option<String>,
    stats_json: String,
    last_error: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    heartbeat_at: Option<String>,
    lease_expires_at: Option<String>,
    updated_at: String,
}

pub(super) fn read_stored(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredProcessingPhaseRecord> {
    Ok(StoredProcessingPhaseRecord {
        data_source_id: row.get(0)?,
        phase: row.get(1)?,
        state: row.get(2)?,
        version: row.get(3)?,
        input_fingerprint: row.get(4)?,
        owner_id: row.get(5)?,
        attempt_id: row.get(6)?,
        stats_json: row.get(7)?,
        last_error: row.get(8)?,
        started_at: row.get(9)?,
        completed_at: row.get(10)?,
        heartbeat_at: row.get(11)?,
        lease_expires_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

pub(super) fn decode_record(
    stored: StoredProcessingPhaseRecord,
) -> DbResult<DataSourceProcessingPhaseRecord> {
    let record = DataSourceProcessingPhaseRecord {
        data_source_id: DataSourceId(stored.data_source_id),
        phase: ProcessingPhase::from_storage(&stored.phase)?,
        state: ProcessingPhaseState::from_storage(&stored.state)?,
        version: u32::try_from(stored.version).map_err(|_| {
            DbError::System("stored processing phase version is invalid".to_string())
        })?,
        input_fingerprint: stored.input_fingerprint,
        owner_id: stored.owner_id,
        attempt_id: stored.attempt_id,
        stats_json: stored.stats_json,
        last_error: stored.last_error,
        started_at: stored.started_at,
        completed_at: stored.completed_at,
        heartbeat_at: stored.heartbeat_at,
        lease_expires_at: stored.lease_expires_at,
        updated_at: stored.updated_at,
    };
    validate_record(&record)?;
    Ok(record)
}
