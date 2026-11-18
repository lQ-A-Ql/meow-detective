//! Shared read-side gate for the timeline projection marker.
//!
//! Cross-source readers skip a source whose marker is explicitly `building` or
//! `deferred`; single-source explicit queries propagate the typed
//! `ProjectionNotReady` error through `From<StorageRuntimeError>`.

use super::TimelineServiceError;
use crate::storage_runtime::StorageRuntimeError;

/// Convert a timeline-projection gate failure into a skip decision.
///
/// Returns `Ok(())` when the source must be skipped (logged here); any other
/// storage error is converted and propagated by the caller with `?`.
pub(super) fn skip_if_not_ready(
    error: StorageRuntimeError,
    source_id: &str,
) -> Result<(), TimelineServiceError> {
    match error {
        StorageRuntimeError::TimelineProjectionNotReady { status, reason, .. } => {
            tracing::warn!(
                source_id = %source_id,
                status = %status,
                reason = reason.as_deref().unwrap_or(""),
                "skipping timeline source whose projection is not ready"
            );
            Ok(())
        }
        other => Err(TimelineServiceError::from(other)),
    }
}
