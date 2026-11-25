use super::TimelineProjectionError;
use std::sync::atomic::{AtomicBool, Ordering};
use timeline::TimelineProjectionPolicy;

const MIB: u64 = 1024 * 1024;

/// Fixed base working set: bounded read buffers, SQLite scratch, and
/// per-invocation bookkeeping outside native ingestion.
const BASE_WORKSPACE_BYTES: u64 = 64 * MIB;
/// Bounded SQLite graph-writer cache inside the base workspace allowance.
pub(super) const GRAPH_SQLITE_CACHE_KIB: i64 = 32 * 1024;

/// Shared engine metadata allowance alongside the separately bounded worker buffers.
const ENGINE_RETAINED_BYTES: u64 = 160 * MIB;

/// Eight application workers, each with bounded input, encoded data and queue slots.
const NATIVE_WORKER_BUFFER_BYTES: u64 = 256 * MIB;

/// Count aggregation, revision publication and batch bookkeeping.
const COUNT_ALLOWANCE_BYTES: u64 = 8 * MIB;

/// Graph projection row/edge buffer allowance kept separate from the file-event
/// native ingestion.
const GRAPH_ALLOWANCE_BYTES: u64 = 32 * MIB;

/// Bounded parallel-write workspace independent of catalog cardinality.
#[derive(Debug, Clone, Default)]
pub(crate) struct WorkspaceEstimate {
    pub required_bytes: u64,
    pub engine_retained_bytes: u64,
}

/// Admission covers the engine, all worker buffers, count aggregation and graph buffers.
pub const fn plan_required_bytes() -> u64 {
    BASE_WORKSPACE_BYTES
        + ENGINE_RETAINED_BYTES
        + NATIVE_WORKER_BUFFER_BYTES
        + COUNT_ALLOWANCE_BYTES
        + GRAPH_ALLOWANCE_BYTES
}

pub(crate) fn estimate(cancel: &AtomicBool) -> Result<WorkspaceEstimate, TimelineProjectionError> {
    active(cancel)?;
    Ok(WorkspaceEstimate {
        required_bytes: plan_required_bytes(),
        engine_retained_bytes: ENGINE_RETAINED_BYTES,
    })
}

impl WorkspaceEstimate {
    pub fn require(&self, policy: TimelineProjectionPolicy) -> Result<(), TimelineProjectionError> {
        let available_bytes = policy.reservation_bytes()?;
        if self.required_bytes > available_bytes {
            return Err(TimelineProjectionError::WorkspaceLimit {
                required_bytes: self.required_bytes,
                available_bytes,
                identity: String::new(),
            });
        }
        Ok(())
    }
}

fn active(cancel: &AtomicBool) -> Result<(), TimelineProjectionError> {
    if cancel.load(Ordering::Acquire) {
        Err(TimelineProjectionError::Cancelled)
    } else {
        Ok(())
    }
}
