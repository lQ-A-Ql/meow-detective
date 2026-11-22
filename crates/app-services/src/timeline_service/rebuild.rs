//! Timeline projection rebuild with per-(case, source) single-flight guard.
//!
//! The segmented write performed by `materialize_file_activity_with_policy` owns
//! the `complete` marker; this module only acquires the source, runs the
//! projection, and re-reads status. A second concurrent rebuild for the same
//! (case_id, source_id) returns `RebuildInProgress` without executing.

use domain::{CaseId, DataSourceId};
use import_core::timeline_projection::materialize_file_activity_with_policy;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use std::{
    collections::HashSet,
    path::Path,
    sync::{atomic::AtomicBool, Mutex, OnceLock},
};
use transport::dto::TimelineProjectionStatusDto;

use super::projection_status::get_timeline_projection_status;
use super::{TimelineProjectionPolicy, TimelineServiceError};
use crate::source_db;

type ActiveRebuildSet = Mutex<HashSet<(String, Option<String>)>>;

static ACTIVE_REBUILDS: OnceLock<ActiveRebuildSet> = OnceLock::new();

pub struct RebuildGuard {
    key: (String, Option<String>),
}

impl Drop for RebuildGuard {
    fn drop(&mut self) {
        if let Some(set) = ACTIVE_REBUILDS.get() {
            let mut locked = set.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            locked.remove(&self.key);
        }
    }
}

/// Acquire the per-(case, source) rebuild single-flight lease without running a
/// rebuild. The returned guard releases the lease on drop; it exists so callers
/// (including tests) can hold the slot while observing the conflict error.
pub fn acquire_rebuild_guard(
    case_id: &CaseId,
    source_id: Option<&DataSourceId>,
) -> Result<RebuildGuard, TimelineServiceError> {
    let key = (case_id.0.clone(), source_id.map(|id| id.0.clone()));
    let set = ACTIVE_REBUILDS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut locked = set.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if locked.iter().any(|(case, source)| {
        case == &key.0 && (source.is_none() || key.1.is_none() || source == &key.1)
    }) {
        return Err(TimelineServiceError::RebuildInProgress { source_id: key.1 });
    }
    locked.insert(key.clone());
    Ok(RebuildGuard { key })
}

pub fn rebuild_timeline_projection(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: Option<&str>,
    policy: TimelineProjectionPolicy,
    cancel: &AtomicBool,
) -> Result<TimelineProjectionStatusDto, TimelineServiceError> {
    let target = data_source_id.map(|raw| DataSourceId(raw.to_owned()));
    let _guard = acquire_rebuild_guard(case_id, target.as_ref())?;
    let repo = DataSourceRepo::new(case_conn);
    let mut rebuilt = false;
    for source in repo.find_by_case(case_id)? {
        if let Some(target) = &target {
            if source.id != *target {
                continue;
            }
        } else if !needs_rebuild(case_root, &source.id)? {
            continue;
        }
        let ready = source_db::open_ready_source_by_id(case_conn, case_root, case_id, &source.id)?;
        let _budget = source_kernel::resource_budget::global_pool().acquire(
            source_kernel::resource_budget::Resources {
                bytes: import_core::timeline_projection::plan_required_bytes(),
                workers: 1,
                ..Default::default()
            },
            Some(cancel),
        )?;
        materialize_file_activity_with_policy(&ready.connection, ready.platform, cancel, policy)?;
        rebuilt = true;
    }
    if let Some(target) = &target {
        if !rebuilt {
            return Err(TimelineServiceError::NotFound(format!(
                "data source '{}' was not found in case '{}'",
                target.0, case_id.0
            )));
        }
    }
    get_timeline_projection_status(case_conn, case_root, case_id)
}

fn needs_rebuild(case_root: &Path, source_id: &DataSourceId) -> Result<bool, TimelineServiceError> {
    let handle = source_db::open_registered_fjall_source_read_only(case_root, source_id)?;
    let snapshot = handle.snapshot();
    let Some(payload) = handle.timeline_projection_state(&snapshot, &source_id.0)? else {
        return Ok(false);
    };
    Ok(matches!(
        payload.status,
        persistence_source::fjall::TimelineProjectionState::Building
            | persistence_source::fjall::TimelineProjectionState::Deferred
    ))
}
