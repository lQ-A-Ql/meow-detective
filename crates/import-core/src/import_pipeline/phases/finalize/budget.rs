use source_kernel::resource_budget::{ResourcePool, Resources};
use std::sync::atomic::AtomicBool;
use timeline::TimelineProjectionPolicy;
use transport::CommandError;

pub(super) fn run<T>(
    pool: &ResourcePool,
    policy: TimelineProjectionPolicy,
    cancel: &AtomicBool,
    operation: impl FnOnce() -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let bytes = policy.reservation_bytes().map_err(|error| {
        CommandError::from_typed_service_error(
            crate::timeline_projection::TimelineProjectionError::from(error),
        )
    })?;
    let permit = pool
        .acquire(
            Resources {
                bytes,
                ..Default::default()
            },
            Some(cancel),
        )
        .map_err(CommandError::from_typed_service_error)?;
    let rss_before_mib = source_kernel::runtime_resources::current_rss_mb();
    let result = operation();
    drop(permit);
    tracing::debug!(
        workspace_bytes = bytes,
        rss_before_mib,
        rss_after_mib = source_kernel::runtime_resources::current_rss_mb(),
        process_lifetime_peak_mib = source_kernel::runtime_resources::peak_rss_mb(),
        completed = result.is_ok(),
        "Timeline projection reservation released"
    );
    result
}

#[cfg(test)]
#[path = "../../../../tests/unit/import_pipeline/phases/projection_budget.rs"]
mod tests;
