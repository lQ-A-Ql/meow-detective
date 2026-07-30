use std::path::Path;

use domain::CaseId;
use transport::dto::CaseSummaryDto;

use crate::state::AppState;

pub(super) async fn restore_enabled_bitlocker_volumes(
    state: &AppState,
    case_root: &Path,
    case: &CaseSummaryDto,
) {
    let state = state.clone();
    let case_root = case_root.to_path_buf();
    let case_id = CaseId(case.id.clone());
    let restore = tauri::async_runtime::spawn_blocking(move || {
        let connection = state.get_connection().map_err(|error| error.to_string())?;
        let context = app_services::bitlocker_service::BitLockerRuntimeContext::new(
            &state.preview_runtime,
            &state.bitlocker_runtime,
            state.bitlocker_key_store.as_ref(),
        );
        app_services::bitlocker_service::restore_enabled_bitlocker_volumes(
            &connection,
            &case_root,
            &case_id,
            context,
        )
        .map_err(|error| error.to_string())
    });
    match restore.await {
        Ok(Ok(summary)) if summary.attempted > 0 => tracing::info!(
            case_id = case.id,
            attempted = summary.attempted,
            restored = summary.restored,
            failed = summary.failed,
            disabled = summary.disabled,
            "Completed persisted BitLocker volume restoration"
        ),
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            tracing::warn!(case_id = case.id, %error, "BitLocker volume restoration could not start")
        }
        Err(error) => {
            tracing::warn!(case_id = case.id, %error, "BitLocker volume restoration worker failed")
        }
    }
}
