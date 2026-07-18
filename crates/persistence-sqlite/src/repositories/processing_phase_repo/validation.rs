use domain::DataSourceId;
use rusqlite::Connection;

use crate::connection::{DbError, DbResult};

use super::types::{
    DataSourceProcessingPhaseRecord, ProcessingPhaseState, ProcessingPhaseTransition,
};

pub(super) fn ensure_derived_source(
    conn: &Connection,
    data_source_id: &DataSourceId,
) -> DbResult<()> {
    let valid: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM data_sources WHERE id = ?1 AND kind = 'ceph_rbd'
         )",
        [&data_source_id.0],
        |row| row.get(0),
    )?;
    if !valid {
        return invalid("processing phases require a Ceph RBD derived data source");
    }
    Ok(())
}

pub(super) fn validate_identity(
    data_source_id: &DataSourceId,
    version: u32,
    input_fingerprint: &str,
) -> DbResult<()> {
    validate_data_source_id(data_source_id)?;
    if version == 0 {
        return invalid("processing phase version must be positive");
    }
    if input_fingerprint.len() != 64
        || input_fingerprint
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return invalid("processing phase input fingerprint must be lowercase SHA-256");
    }
    Ok(())
}

pub(super) fn validate_data_source_id(data_source_id: &DataSourceId) -> DbResult<()> {
    if !valid_text(&data_source_id.0) {
        return invalid("processing phase data-source ID is invalid");
    }
    Ok(())
}

pub(super) fn validate_transition_payload(
    transition: ProcessingPhaseTransition<'_>,
) -> DbResult<()> {
    if matches!(
        transition.state,
        ProcessingPhaseState::Pending | ProcessingPhaseState::Running
    ) {
        return invalid("processing phase completion requires a terminal state");
    }
    validate_stats_json(transition.stats_json)?;
    if transition
        .last_error
        .is_some_and(|value| !valid_text(value))
    {
        return invalid("processing phase error text is invalid");
    }
    match transition.state {
        ProcessingPhaseState::Ready if transition.last_error.is_some() => {
            invalid("ready processing phases cannot retain an error")
        }
        ProcessingPhaseState::Failed if transition.last_error.is_none() => {
            invalid("failed processing phases require an error")
        }
        _ => Ok(()),
    }
}

pub(super) fn validate_attempt(owner_id: &str, attempt_id: &str) -> DbResult<()> {
    if !valid_text(owner_id) || !valid_text(attempt_id) {
        return invalid("processing phase owner or attempt ID is invalid");
    }
    Ok(())
}

pub(super) fn validate_transition_identity(
    current: &DataSourceProcessingPhaseRecord,
    version: u32,
    input_fingerprint: &str,
) -> DbResult<()> {
    if current.version != version || current.input_fingerprint != input_fingerprint {
        return invalid("processing phase transition has a stale version or input fingerprint");
    }
    Ok(())
}

pub(super) fn transition_matches(
    current: &DataSourceProcessingPhaseRecord,
    owner_id: &str,
    attempt_id: &str,
    transition: ProcessingPhaseTransition<'_>,
) -> bool {
    current.state == transition.state
        && current.owner_id.as_deref() == Some(owner_id)
        && current.attempt_id.as_deref() == Some(attempt_id)
        && current.stats_json == transition.stats_json
        && current.last_error.as_deref() == transition.last_error
}

pub(super) fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains('\0')
}

pub(super) fn validate_record(record: &DataSourceProcessingPhaseRecord) -> DbResult<()> {
    validate_identity(
        &record.data_source_id,
        record.version,
        &record.input_fingerprint,
    )?;
    validate_stats_json(&record.stats_json)?;
    if !valid_text(&record.updated_at)
        || optional_invalid(record.owner_id.as_deref())
        || optional_invalid(record.attempt_id.as_deref())
        || optional_invalid(record.last_error.as_deref())
        || optional_invalid(record.started_at.as_deref())
        || optional_invalid(record.completed_at.as_deref())
        || optional_invalid(record.heartbeat_at.as_deref())
        || optional_invalid(record.lease_expires_at.as_deref())
    {
        return invalid("stored processing phase metadata is invalid");
    }
    if !state_metadata_is_valid(record) {
        return invalid("stored processing phase state metadata is inconsistent");
    }
    Ok(())
}

fn state_metadata_is_valid(record: &DataSourceProcessingPhaseRecord) -> bool {
    let has_owner = record.owner_id.is_some() && record.attempt_id.is_some();
    let active = record.heartbeat_at.is_some();
    match record.state {
        ProcessingPhaseState::Pending => {
            !has_owner
                && record.started_at.is_none()
                && record.completed_at.is_none()
                && !active
                && record.lease_expires_at.is_none()
                && record.last_error.is_none()
        }
        ProcessingPhaseState::Running => {
            has_owner
                && record.started_at.is_some()
                && record.completed_at.is_none()
                && active
                && record.lease_expires_at.is_some()
                && record.last_error.is_none()
        }
        ProcessingPhaseState::Ready => {
            terminal_metadata_is_valid(record, has_owner, active) && record.last_error.is_none()
        }
        ProcessingPhaseState::Failed => {
            terminal_metadata_is_valid(record, has_owner, active) && record.last_error.is_some()
        }
        ProcessingPhaseState::Deferred => terminal_metadata_is_valid(record, has_owner, active),
    }
}

fn terminal_metadata_is_valid(
    record: &DataSourceProcessingPhaseRecord,
    has_owner: bool,
    active: bool,
) -> bool {
    has_owner && record.completed_at.is_some() && active && record.lease_expires_at.is_none()
}

fn optional_invalid(value: Option<&str>) -> bool {
    value.is_some_and(|value| !valid_text(value))
}

fn validate_stats_json(stats_json: &str) -> DbResult<()> {
    if !serde_json::from_str::<serde_json::Value>(stats_json).is_ok_and(|value| value.is_object()) {
        return invalid("processing phase stats must be a JSON object");
    }
    Ok(())
}

pub(super) fn invalid<T>(message: impl Into<String>) -> DbResult<T> {
    Err(DbError::System(message.into()))
}
