use domain::DataSourcePlatform;
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

use timeline::TimelineProjectionPolicy;

mod error;
mod fjall;
mod graph;
mod progress;
mod timing;
pub use progress::{TimelineProgressCallback, TimelineWork};
pub(crate) mod workspace;

pub use error::TimelineProjectionError;
pub use timing::TimelineWriteTimings;

pub use workspace::plan_required_bytes;

use crate::timeline_projection::fjall as projection_fjall;

/// SQLite `timeline_projection_meta` key for the file-activity projection row.
pub(crate) const FILE_PROJECTION_KEY: &str = "file_activity_v2";

/// SQLite `timeline_projection_meta` key for the graph projection row.
pub(crate) const GRAPH_PROJECTION_KEY: &str = "timeline_graph_v3";

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
    /// Constant plan bytes (not measured) compared against the policy cap.
    pub workspace_required_bytes: u64,
    /// Policy reservation bytes the plan was checked against.
    pub workspace_available_bytes: u64,
    /// Native ingestion batches completed during delete and insert work.
    pub workspace_segments: u64,
    /// Largest committed segment payload in bytes.
    pub workspace_max_segment_bytes: u64,
    /// Engine metadata allowance in the bounded parallel workspace.
    pub workspace_engine_retained_bytes: u64,
    /// Number of completed native write batches.
    pub commit_count: u64,
    pub native_writes: persistence_source::fjall::NativeWriteReport,
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
    materialize_file_activity_with_progress(conn, platform, cancel_token, policy, None)
}

pub fn materialize_file_activity_with_progress(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
    policy: TimelineProjectionPolicy,
    progress: Option<&TimelineProgressCallback<'_>>,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    policy.validate()?;
    ensure_not_cancelled(cancel_token)?;
    let identity = projection_fjall::identity(conn, platform, cancel_token, progress)?;
    projection_fjall::materialize(conn, platform, cancel_token, &identity, policy, progress)
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
    projection_fjall::materialize(conn, platform, cancel_token, input_identity, policy, None)
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
