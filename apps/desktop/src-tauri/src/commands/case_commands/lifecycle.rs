use app_services::case_service;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, State};
use transport::{
    commands::{CreateCaseRequest, OpenCaseRequest},
    dto::CaseSummaryDto,
    CommandError,
};

use super::recent::remember_recent_case;
use super::recovery::recover_interrupted_jobs;
use super::transition::begin_active_case_transition;
use crate::{events::event_bridge, state::AppState};

fn init_case_db(state: &AppState) -> Result<(), CommandError> {
    // AppState methods are typed `Result<_, String>` today, so this stays on the
    // substring-matching fallback path.
    state
        .init_db_pragmas()
        .map_err(CommandError::from_service_error)
}

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

#[tauri::command]
pub async fn create_case(
    state: State<'_, AppState>,
    app: AppHandle,
    request: CreateCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let root = PathBuf::from(&request.case_root);
    let name = request.name;
    let examiner = request.examiner;
    let active = tauri::async_runtime::spawn_blocking(move || {
        case_service::create_case(&root, &name, examiner.as_deref())
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;
    let active_case_root = active.case_root.clone();
    let dto = meta_to_dto(&active.meta);
    let transition = match begin_active_case_transition(&app_state, active, Duration::from_secs(5))
    {
        Ok(transition) => transition,
        Err(error) => {
            rollback_created_case(active_case_root).await;
            return Err(error);
        }
    };
    if let Err(error) = initialize_and_remember(&app_state, &active_case_root, &dto) {
        transition.rollback(&app_state, Duration::from_secs(5));
        rollback_created_case(active_case_root).await;
        return Err(error);
    }
    transition.commit(&app_state);
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub async fn open_case(
    state: State<'_, AppState>,
    app: AppHandle,
    request: OpenCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let root = PathBuf::from(&request.case_root);
    let open_root = root.clone();
    let active = tauri::async_runtime::spawn_blocking(move || {
        case_service::open_case(&open_root).map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;
    let dto = meta_to_dto(&active.meta);
    let transition = begin_active_case_transition(&app_state, active, Duration::from_secs(5))?;
    if let Err(error) = initialize_and_remember(&app_state, &root, &dto) {
        transition.rollback(&app_state, Duration::from_secs(5));
        return Err(error);
    }
    restore_enabled_bitlocker_volumes(&app_state, &root, &dto).await;
    transition.commit(&app_state);
    recover_interrupted_jobs(&app_state);
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub async fn create_analysis_demo_case(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<CaseSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let case_root = std::env::temp_dir().join("Meow_Detective-analysis-demo");
    let create_root = case_root.clone();
    let active = tauri::async_runtime::spawn_blocking(move || {
        if create_root.exists() {
            std::fs::remove_dir_all(&create_root).map_err(|error| {
                CommandError::internal(format!("Failed to reset analysis demo case: {error}"))
            })?;
        }
        std::fs::create_dir_all(&create_root).map_err(|error| {
            CommandError::internal(format!("Failed to create analysis demo root: {error}"))
        })?;
        let active = case_service::create_case(&create_root, "Analysis Demo", Some("Codex Demo"))
            .map_err(CommandError::from_typed_service_error)?;
        app_services::analysis_service::seed_analysis_demo_data(&active)
            .map_err(CommandError::from_typed_service_error)?;
        Ok::<_, CommandError>(active)
    })
    .await
    .map_err(CommandError::from_join_error)??;
    let active_case_root = active.case_root.clone();
    let dto = meta_to_dto(&active.meta);
    let transition = match begin_active_case_transition(&app_state, active, Duration::from_secs(5))
    {
        Ok(transition) => transition,
        Err(error) => {
            rollback_created_case(active_case_root).await;
            return Err(error);
        }
    };
    if let Err(error) = initialize_and_remember(&app_state, &active_case_root, &dto) {
        transition.rollback(&app_state, Duration::from_secs(5));
        rollback_created_case(active_case_root).await;
        return Err(error);
    }
    transition.commit(&app_state);
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub fn get_current_case(state: State<AppState>) -> Result<Option<CaseSummaryDto>, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    match guard.as_ref() {
        Some(active) => Ok(Some(meta_to_dto(&active.meta))),
        None => Ok(None),
    }
}

fn initialize_and_remember(
    state: &AppState,
    case_root: &std::path::Path,
    dto: &CaseSummaryDto,
) -> Result<(), CommandError> {
    init_case_db(state)?;
    remember_recent_case(case_root, dto)
}

async fn restore_enabled_bitlocker_volumes(
    state: &AppState,
    case_root: &std::path::Path,
    case: &CaseSummaryDto,
) {
    let state = state.clone();
    let case_root = case_root.to_path_buf();
    let case_id = domain::CaseId(case.id.clone());
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

async fn rollback_created_case(case_root: PathBuf) {
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
