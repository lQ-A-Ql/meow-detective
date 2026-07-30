use std::{path::Path, path::PathBuf};

use app_services::case_service;
use transport::{dto::CaseSummaryDto, CommandError};

use super::recent::remember_recent_case;
use crate::state::AppState;

pub(super) fn meta_to_dto(meta: &domain::CaseMeta) -> CaseSummaryDto {
    CaseSummaryDto {
        id: meta.id.0.clone(),
        name: meta.name.clone(),
        number: meta.number.clone(),
        examiner: meta.examiner.clone(),
        created_at: meta.created_at.to_rfc3339(),
        updated_at: meta.updated_at.to_rfc3339(),
    }
}

pub(super) fn initialize_and_remember(
    state: &AppState,
    case_root: &Path,
    dto: &CaseSummaryDto,
) -> Result<(), CommandError> {
    state
        .init_db_pragmas()
        .map_err(CommandError::from_service_error)?;
    remember_recent_case(case_root, dto)
}

pub(super) async fn rollback_created_case(case_root: PathBuf) {
    let cleanup_root = case_root.clone();
    let cleanup =
        tauri::async_runtime::spawn_blocking(move || case_service::delete_case(&cleanup_root))
            .await;
    match cleanup {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::error!(
            case_root = %case_root.display(),
            %error,
            "Failed to roll back case creation"
        ),
        Err(error) => tracing::error!(
            case_root = %case_root.display(),
            %error,
            "Case creation rollback worker failed"
        ),
    }
}
