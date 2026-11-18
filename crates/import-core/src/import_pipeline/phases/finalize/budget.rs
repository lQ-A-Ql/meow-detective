use source_kernel::resource_budget::{ResourceError, ResourcePool, Resources};
use std::sync::atomic::AtomicBool;
use timeline::TimelineProjectionPolicy;

use crate::timeline_projection::TimelineProjectionError;

/// Acquire the planned (constant) bytes from the process pool, run the
/// projection, and release the permit. Capacity-class pool admission failures
/// map to `TimelineProjectionError::WorkspaceLimit` (code
/// `TIMELINE_WORKSPACE_LIMIT`); cancellation and fatal storage errors propagate
/// unchanged so finalize can match on the typed error instead of strings.
pub(super) fn run<T>(
    pool: &ResourcePool,
    policy: TimelineProjectionPolicy,
    planned_bytes: u64,
    cancel: &AtomicBool,
    operation: impl FnOnce() -> Result<T, TimelineProjectionError>,
) -> Result<T, TimelineProjectionError> {
    policy.validate()?;
    let permit = pool
        .acquire(
            Resources {
                bytes: planned_bytes,
                ..Default::default()
            },
            Some(cancel),
        )
        .map_err(|error| map_resource_error(error, planned_bytes, pool.limits().bytes))?;
    let rss_before_mib = source_kernel::runtime_resources::current_rss_mb();
    let result = operation();
    drop(permit);
    tracing::debug!(
        planned_bytes,
        pool_limit_bytes = pool.limits().bytes,
        rss_before_mib,
        rss_after_mib = source_kernel::runtime_resources::current_rss_mb(),
        process_lifetime_peak_mib = source_kernel::runtime_resources::peak_rss_mb(),
        completed = result.is_ok(),
        "Timeline projection reservation released"
    );
    result
}

fn map_resource_error(
    error: ResourceError,
    required_bytes: u64,
    available_bytes: u64,
) -> TimelineProjectionError {
    match error {
        ResourceError::Limit => TimelineProjectionError::WorkspaceLimit {
            required_bytes,
            available_bytes,
            projected_events: 0,
            removed_events: 0,
            graph_events: 0,
            mutation_bytes: 0,
            mutation_entries: 0,
            identity: String::new(),
        },
        ResourceError::Cancelled => TimelineProjectionError::Cancelled,
        ResourceError::Busy => {
            TimelineProjectionError::Other("timeline projection resource pool is busy".into())
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/import_pipeline/phases/projection_budget.rs"]
mod tests;
