use super::TimelineProjectionError;
use std::sync::atomic::{AtomicBool, Ordering};
use timeline::TimelineProjectionPolicy;

const MIB: u64 = 1024 * 1024;

/// Fixed base working set: bounded read buffers, SQLite scratch, and
/// per-invocation bookkeeping outside the segmented engine writes.
const BASE_WORKSPACE_BYTES: u64 = 64 * MIB;

/// Engine-retained budget held as a constant pending calibration (待实测校准):
/// segmented-commit retention has not been measured yet, so this value is not
/// derived from catalog/event counts. Replace with a measured constant once a
/// segmented-commit benchmark lands; do not tune it from case size in the
/// meantime.
const ENGINE_RETAINED_BYTES: u64 = 160 * MIB;

/// One in-flight insert segment bounded by `SegmentLimits::default_limits()`
/// (64 MiB / 100 000 records).
const SEGMENT_BYTES: u64 = 64 * MIB;

/// Count-delta map, revision bump, and per-segment bookkeeping allowance.
const COUNT_ALLOWANCE_BYTES: u64 = 8 * MIB;

/// Graph projection row/edge buffer allowance kept separate from the file-event
/// segmented path.
const GRAPH_ALLOWANCE_BYTES: u64 = 32 * MIB;

/// Per-record container allocation inside one segment.
const SEGMENT_RECORD_CONTAINER_BYTES: u64 =
    std::mem::size_of::<persistence_source::fjall::TimelineRecord>() as u64;
const SEGMENT_MAX_RECORDS: u64 = 100_000;

/// Constant segment plan. The estimate is intentionally decoupled from the
/// catalog/event count so a large case cannot be rejected before a single write;
/// capacity admission compares this fixed plan against the policy cap.
#[derive(Debug, Clone, Default)]
pub(crate) struct WorkspaceEstimate {
    pub projected_events: u64,
    pub removed_events: u64,
    pub graph_events: u64,
    pub mutation_bytes: u64,
    pub mutation_entries: u64,
    pub required_bytes: u64,
    pub engine_retained_bytes: u64,
}

/// Fixed plan bytes: BASE + ENGINE_RETAINED + SEGMENT + segment container +
/// COUNT_ALLOWANCE + GRAPH_ALLOWANCE. Compile-time constant so the plan never
/// depends on how many files or events the case holds.
pub const fn plan_required_bytes() -> u64 {
    let segment_container = SEGMENT_RECORD_CONTAINER_BYTES * SEGMENT_MAX_RECORDS;
    BASE_WORKSPACE_BYTES
        + ENGINE_RETAINED_BYTES
        + SEGMENT_BYTES
        + segment_container
        + COUNT_ALLOWANCE_BYTES
        + GRAPH_ALLOWANCE_BYTES
}

pub(crate) fn estimate(cancel: &AtomicBool) -> Result<WorkspaceEstimate, TimelineProjectionError> {
    active(cancel)?;
    Ok(WorkspaceEstimate {
        required_bytes: plan_required_bytes(),
        engine_retained_bytes: ENGINE_RETAINED_BYTES,
        ..Default::default()
    })
}

impl WorkspaceEstimate {
    pub fn require(&self, policy: TimelineProjectionPolicy) -> Result<(), TimelineProjectionError> {
        let available_bytes = policy.reservation_bytes()?;
        if self.required_bytes > available_bytes {
            return Err(TimelineProjectionError::WorkspaceLimit {
                required_bytes: self.required_bytes,
                available_bytes,
                projected_events: self.projected_events,
                removed_events: self.removed_events,
                graph_events: self.graph_events,
                mutation_bytes: self.mutation_bytes,
                mutation_entries: self.mutation_entries,
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
