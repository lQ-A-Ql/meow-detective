//! Read-side case-wide timeline projection status enumeration.
//!
//! Every data source of the case is listed, including sources whose storage
//! cannot be opened (logged and skipped instead of failing the whole query).
//! A missing marker means `complete` for legacy-case compatibility; the overall
//! state is the worst per-source state (deferred > building > complete).

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{
    TimelineProjectionSourceStatusDto, TimelineProjectionStateDto, TimelineProjectionStatusDto,
};

use super::TimelineServiceError;
use crate::source_db;

pub fn get_timeline_projection_status(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<TimelineProjectionStatusDto, TimelineServiceError> {
    let repo = DataSourceRepo::new(case_conn);
    let mut sources = Vec::new();
    let mut overall = TimelineProjectionStateDto::Complete;
    for source in repo.find_by_case(case_id)? {
        match read_source_status(case_root, &source.id)? {
            Some(row) => {
                overall = worst_state(overall, row.state);
                sources.push(row);
            }
            None => continue,
        }
    }
    Ok(TimelineProjectionStatusDto {
        state: overall,
        sources,
    })
}

fn read_source_status(
    case_root: &Path,
    source_id: &DataSourceId,
) -> Result<Option<TimelineProjectionSourceStatusDto>, TimelineServiceError> {
    let handle = match source_db::open_registered_fjall_source_read_only(case_root, source_id) {
        Ok(handle) => handle,
        Err(error) => {
            tracing::warn!(
                source_id = %source_id.0,
                error = %error,
                "skipping unavailable source while reading timeline projection status"
            );
            return Ok(None);
        }
    };
    let snapshot = handle.snapshot();
    let Some(payload) = handle.timeline_projection_state(&snapshot, &source_id.0)? else {
        return Ok(Some(TimelineProjectionSourceStatusDto {
            data_source_id: source_id.0.clone(),
            state: TimelineProjectionStateDto::Complete,
            identity: None,
            inserted_events: 0,
            required_bytes: None,
            available_bytes: None,
            reason: None,
            updated_at: None,
        }));
    };
    Ok(Some(TimelineProjectionSourceStatusDto {
        data_source_id: source_id.0.clone(),
        state: map_state(payload.status),
        identity: Some(payload.identity),
        inserted_events: payload.inserted_events,
        required_bytes: Some(payload.required_bytes),
        available_bytes: Some(payload.available_bytes),
        reason: payload.reason,
        updated_at: Some(payload.updated_at),
    }))
}

fn map_state(
    state: persistence_source::fjall::TimelineProjectionState,
) -> TimelineProjectionStateDto {
    match state {
        persistence_source::fjall::TimelineProjectionState::Complete => {
            TimelineProjectionStateDto::Complete
        }
        persistence_source::fjall::TimelineProjectionState::Building => {
            TimelineProjectionStateDto::Building
        }
        persistence_source::fjall::TimelineProjectionState::Deferred => {
            TimelineProjectionStateDto::Deferred
        }
    }
}

fn worst_state(
    left: TimelineProjectionStateDto,
    right: TimelineProjectionStateDto,
) -> TimelineProjectionStateDto {
    let rank = |state: TimelineProjectionStateDto| match state {
        TimelineProjectionStateDto::Deferred => 2,
        TimelineProjectionStateDto::Building => 1,
        TimelineProjectionStateDto::Complete => 0,
    };
    if rank(left) >= rank(right) {
        left
    } else {
        right
    }
}
