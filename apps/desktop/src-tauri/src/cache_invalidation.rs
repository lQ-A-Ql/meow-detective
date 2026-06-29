//! Backend-side cache invalidation listeners.
//!
//! Listens for lifecycle events that can make ephemeral preview caches stale
//! (for example, after a data source finishes importing) and clears the
//! runtime-cache and E01 reader cache for the affected case.

use tauri::{AppHandle, Listener, Manager};
use transport::events::TOPIC_DATA_SOURCE_IMPORTED;

use crate::state::AppState;

/// Payload emitted with `data-source-imported` events.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataSourceImportedPayload {
    case_id: String,
}

/// Register backend listeners that invalidate preview caches on lifecycle events.
pub fn register(app: AppHandle) {
    let app_clone = app.clone();
    app.listen(TOPIC_DATA_SOURCE_IMPORTED, move |event| {
        let payload: DataSourceImportedPayload = match serde_json::from_str(event.payload()) {
            Ok(p) => p,
            Err(error) => {
                tracing::warn!(%error, "Ignoring malformed data-source-imported event");
                return;
            }
        };
        clear_preview_caches_for_case(&app_clone, &payload.case_id);
    });
}

fn clear_preview_caches_for_case(app: &AppHandle, case_id: &str) {
    let state = app.state::<AppState>();

    // Only clear caches if the imported case is currently active. Import jobs
    // for a closed case should not affect whatever case (if any) is open now.
    let active_case_id = {
        let guard = match state.active_case.lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::error!(%error, "Failed to lock active case for cache invalidation");
                return;
            }
        };
        guard.as_ref().map(|active| active.meta.id.0.clone())
    };

    let Some(active_case_id) = active_case_id else {
        tracing::debug!("No active case; skipping cache invalidation");
        return;
    };

    if active_case_id != case_id {
        tracing::debug!(
            active_case_id = %active_case_id,
            imported_case_id = %case_id,
            "Imported case is not active; skipping cache invalidation"
        );
        return;
    }

    if let Err(error) = state.clear_runtime_cache_for_case(case_id) {
        tracing::error!(%error, "Failed to clear runtime cache after import");
    }
    app_services::file_service::clear_e01_reader_cache_for_case(case_id);
    tracing::info!(case_id = %case_id, "Cleared preview caches after import");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_deserializes_with_camel_case() {
        let json = r#"{"caseId":"case-123","dataSourceId":"ds-456","name":"C","kind":"E01","jobId":"job-789"}"#;
        let payload: DataSourceImportedPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.case_id, "case-123");
    }
}
