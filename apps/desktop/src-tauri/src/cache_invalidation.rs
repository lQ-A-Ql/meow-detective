//! Backend-side cache invalidation listeners.
//!
//! Listens for lifecycle events that can make ephemeral preview caches stale
//! (for example, after a data source finishes importing) and clears the
//! runtime-cache and E01 reader cache for the affected case.

use std::time::Duration;
use tauri::{AppHandle, Listener, Manager};
use transport::events::{EventEnvelope, TOPIC_DATA_SOURCE_IMPORTED};

use crate::state::AppState;

/// Payload emitted with `data-source-imported` events.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataSourceImportedPayload {
    case_id: String,
    data_source_id: String,
}

/// Register backend listeners that invalidate preview caches on lifecycle events.
pub fn register(app: AppHandle) {
    let app_clone = app.clone();
    app.listen(TOPIC_DATA_SOURCE_IMPORTED, move |event| {
        let envelope: EventEnvelope<DataSourceImportedPayload> =
            match serde_json::from_str(event.payload()) {
                Ok(envelope) => envelope,
                Err(error) => {
                    tracing::warn!(%error, "Ignoring malformed data-source-imported event");
                    return;
                }
            };
        let payload = envelope.payload;
        clear_preview_caches_for_source(&app_clone, &payload.case_id, &payload.data_source_id);
    });
}

fn clear_preview_caches_for_source(app: &AppHandle, case_id: &str, data_source_id: &str) {
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

    let drained =
        match state.retire_preview_source(case_id, data_source_id, Duration::from_secs(30)) {
            Ok(drained) => drained,
            Err(error) => {
                tracing::error!(%error, "Failed to retire preview runtime after import");
                return;
            }
        };
    if !drained {
        tracing::error!(
            case_id = %case_id,
            data_source_id = %data_source_id,
            "Preview runtime remains retired because active reads did not drain after import"
        );
        return;
    }
    if let Err(error) = state.clear_runtime_cache_for_case(case_id) {
        tracing::error!(%error, "Failed to clear runtime cache after import");
    }
    app_services::file_service::clear_e01_reader_cache_for_case(case_id);
    if let Err(error) = state.reactivate_preview_source(case_id, data_source_id) {
        tracing::error!(%error, "Failed to reactivate preview runtime after import");
        return;
    }
    tracing::info!(
        case_id = %case_id,
        data_source_id = %data_source_id,
        "Cleared preview caches after import"
    );
}

#[cfg(test)]
#[path = "../tests/unit/cache_invalidation.rs"]
mod tests;
