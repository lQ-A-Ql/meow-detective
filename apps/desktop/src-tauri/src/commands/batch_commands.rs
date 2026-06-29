use tauri::State;
use transport::dto::batch::{BatchJobDto, BatchPlanDto, BatchResourceLimitsDto, BatchResumeDto};
use transport::CommandError;

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn create_batch_plan(
    state: State<'_, AppState>,
    name: String,
    data_source_ids: Vec<String>,
    phases: Vec<String>,
    resource_limits: BatchResourceLimitsDto,
) -> Result<BatchJobDto, CommandError> {
    if name.trim().is_empty() || name.len() > 200 {
        return Err(CommandError::invalid_input(
            "Batch name must be 1-200 characters",
        ));
    }
    if data_source_ids.is_empty() {
        return Err(CommandError::invalid_input(
            "At least one data source is required",
        ));
    }
    if phases.is_empty() {
        return Err(CommandError::invalid_input(
            "At least one phase is required",
        ));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        let plan = BatchPlanDto {
            data_source_refs: data_source_ids,
            phases,
            resource_limits,
        };
        app_services::batch_service::create_and_persist_batch(&conn, &active.case_id, &name, plan)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn start_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    if batch_id.trim().is_empty() {
        return Err(CommandError::invalid_input("batch_id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::batch_service::start_batch(&conn, &batch_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn pause_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    if batch_id.trim().is_empty() {
        return Err(CommandError::invalid_input("batch_id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::batch_service::pause_batch(&conn, &batch_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn resume_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    if batch_id.trim().is_empty() {
        return Err(CommandError::invalid_input("batch_id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        let resume = BatchResumeDto {
            batch_id,
            resource_limits: None,
        };
        app_services::batch_service::resume_batch(&conn, resume)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn cancel_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    if batch_id.trim().is_empty() {
        return Err(CommandError::invalid_input("batch_id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::batch_service::cancel_batch(&conn, &batch_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_batch_job(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    if batch_id.trim().is_empty() {
        return Err(CommandError::invalid_input("batch_id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::batch_service::get_batch_status(&conn, &batch_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn list_batch_jobs(state: State<'_, AppState>) -> Result<Vec<BatchJobDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::batch_service::list_batch_jobs(&conn, &active.case_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_services::case_service;
    use uuid::Uuid;

    #[test]
    fn batch_commands_require_active_case() {
        let state = AppState::default();
        // Run synchronously using the blocking helper to avoid async runtime in unit test.
        let result = require_active_case(&state);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "NO_ACTIVE_CASE");
    }

    #[test]
    fn batch_plan_roundtrip_persists_job() {
        let root =
            std::env::temp_dir().join(format!("forensics-batch-command-test-{}", Uuid::new_v4()));
        let active = case_service::create_case(&root, "Batch Command", Some("Codex Test")).unwrap();
        let case_id = active.meta.id.0.clone();
        let state = AppState::default();
        *state.active_case.lock().unwrap() = Some(active);
        state.init_db_pragmas().unwrap();

        let conn = state.get_connection().unwrap();
        let job = app_services::batch_service::create_and_persist_batch(
            &conn,
            &case_id,
            "test-plan",
            BatchPlanDto {
                data_source_refs: vec!["ds-1".to_string()],
                phases: vec!["Mount".to_string(), "Catalog".to_string()],
                resource_limits: BatchResourceLimitsDto {
                    max_memory_mb: Some(1024),
                    max_threads: Some(2),
                },
            },
        )
        .unwrap();

        assert_eq!(job.label, "test-plan");
        assert_eq!(job.phases.len(), 2);

        let listed = app_services::batch_service::list_batch_jobs(&conn, &case_id).unwrap();
        assert_eq!(listed.len(), 1);

        state.clear_db_state().unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    fn state_as_managed(state: &AppState) -> tauri::State<'_, AppState> {
        // `State<'_, T>` is a newtype around `&T`; this transmute is sound
        // because the test keeps `state` alive for the duration of the call.
        unsafe { std::mem::transmute(state) }
    }

    fn setup_active_case_with_batch_job() -> (std::path::PathBuf, AppState, String) {
        let root =
            std::env::temp_dir().join(format!("forensics-batch-stub-test-{}", Uuid::new_v4()));
        let active = case_service::create_case(&root, "Batch Stub", Some("Codex Test")).unwrap();
        let case_id = active.meta.id.0.clone();
        let state = AppState::default();
        *state.active_case.lock().unwrap() = Some(active);
        state.init_db_pragmas().unwrap();

        let conn = state.get_connection().unwrap();
        let job = app_services::batch_service::create_and_persist_batch(
            &conn,
            &case_id,
            "stub-plan",
            BatchPlanDto {
                data_source_refs: vec!["ds-1".to_string()],
                phases: vec!["Mount".to_string()],
                resource_limits: BatchResourceLimitsDto {
                    max_memory_mb: None,
                    max_threads: None,
                },
            },
        )
        .unwrap();

        (root, state, job.id)
    }

    #[test]
    fn start_batch_returns_unsupported_stub() {
        let (root, state, batch_id) = setup_active_case_with_batch_job();
        let err = tauri::async_runtime::block_on(start_batch(state_as_managed(&state), batch_id))
            .unwrap_err();
        assert_eq!(err.code, "UNSUPPORTED");
        assert!(err.message.to_ascii_lowercase().contains("not supported"));
        state.clear_db_state().unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn pause_batch_returns_unsupported_stub() {
        let (root, state, batch_id) = setup_active_case_with_batch_job();
        let err = tauri::async_runtime::block_on(pause_batch(state_as_managed(&state), batch_id))
            .unwrap_err();
        assert_eq!(err.code, "UNSUPPORTED");
        assert!(err.message.to_ascii_lowercase().contains("not supported"));
        state.clear_db_state().unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn resume_batch_returns_unsupported_stub() {
        let (root, state, batch_id) = setup_active_case_with_batch_job();
        let err = tauri::async_runtime::block_on(resume_batch(state_as_managed(&state), batch_id))
            .unwrap_err();
        assert_eq!(err.code, "UNSUPPORTED");
        assert!(err.message.to_ascii_lowercase().contains("not supported"));
        state.clear_db_state().unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn cancel_batch_returns_unsupported_stub() {
        let (root, state, batch_id) = setup_active_case_with_batch_job();
        let err = tauri::async_runtime::block_on(cancel_batch(state_as_managed(&state), batch_id))
            .unwrap_err();
        assert_eq!(err.code, "UNSUPPORTED");
        assert!(err.message.to_ascii_lowercase().contains("not supported"));
        state.clear_db_state().unwrap();
        std::fs::remove_dir_all(root).ok();
    }
}
