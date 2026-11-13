use domain::{DataSourcePlatform, FileEntry};
use persistence_source::repositories::timeline_repo::TimelineRepo;
use rayon::prelude::*;
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

use timeline::TimelineProjectionPolicy;

mod error;
mod fjall;
mod timing;
mod workspace;

pub use error::TimelineProjectionError;
pub use timing::TimelineWriteTimings;

use crate::timeline_projection::fjall as projection_fjall;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineProjectionStats {
    pub inserted_count: u64,
    pub elapsed_ms: u128,
    pub events_elapsed_ms: u128,
    pub graph_elapsed_ms: u128,
    pub timings: TimelineWriteTimings,
    pub already_projected: bool,
    pub graph_complete: bool,
    pub warnings: Vec<String>,
    pub workspace_required_bytes: u64,
    pub workspace_available_bytes: u64,
    pub workspace_mutation_bytes: u64,
    pub workspace_projected_events: u64,
    pub workspace_graph_events: u64,
}

pub fn project_and_store_file_activity(
    conn: &Connection,
    files: &[FileEntry],
) -> Result<u64, TimelineProjectionError> {
    let events = files
        .par_iter()
        .flat_map_iter(timeline::project_file_activity)
        .collect::<Vec<_>>();
    let count = events.len() as u64;
    if !events.is_empty() {
        TimelineRepo::new(conn).insert_batch(&events)?;
    }
    Ok(count)
}

pub fn materialize_file_activity(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    materialize_file_activity_with_policy(
        conn,
        platform,
        cancel_token,
        TimelineProjectionPolicy::default(),
    )
}

pub fn materialize_file_activity_with_policy(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
    policy: TimelineProjectionPolicy,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    policy.validate()?;
    ensure_not_cancelled(cancel_token)?;
    let identity = projection_fjall::identity(conn, platform, cancel_token)?;
    materialize_file_activity_with_identity_and_policy(
        conn,
        platform,
        cancel_token,
        &identity,
        policy,
    )
}

pub fn materialize_file_activity_with_identity(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
    input_identity: &str,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    materialize_file_activity_with_identity_and_policy(
        conn,
        platform,
        cancel_token,
        input_identity,
        TimelineProjectionPolicy::default(),
    )
}

pub fn materialize_file_activity_with_identity_and_policy(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
    input_identity: &str,
    policy: TimelineProjectionPolicy,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    ensure_not_cancelled(cancel_token)?;
    policy.validate()?;
    if input_identity.trim().is_empty() {
        return Err(TimelineProjectionError::InvalidInput(
            "timeline projection input identity must not be empty".into(),
        ));
    }
    projection_fjall::materialize(conn, platform, cancel_token, input_identity, policy)
}

pub fn materialize_file_activity_unknown(
    conn: &Connection,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    materialize_file_activity(conn, DataSourcePlatform::Unknown, &AtomicBool::new(false))
}

pub fn materialize_file_activity_unknown_with_cancel(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    materialize_file_activity(conn, DataSourcePlatform::Unknown, cancel_token)
}

pub fn materialize_file_activity_unknown_with_cancel_and_identity(
    conn: &Connection,
    cancel_token: &AtomicBool,
    input_identity: &str,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    materialize_file_activity_with_identity(
        conn,
        DataSourcePlatform::Unknown,
        cancel_token,
        input_identity,
    )
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), TimelineProjectionError> {
    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        Err(TimelineProjectionError::Cancelled)
    } else {
        Ok(())
    }
}
