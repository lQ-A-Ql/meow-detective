use app_services::case_service;
use std::path::PathBuf;
use tauri::State;
use transport::{
    commands::{
        CreateCaseRequest, DeleteCaseRequest, DeleteDataSourceRequest, OpenCaseRequest,
        RenameDataSourceRequest,
    },
    dto::{CaseMetricsDto, CaseSummaryDto, DataSourceSummaryDto, RecentCaseDto, RecentObjectDto},
    CommandError,
};

use crate::state::AppState;

fn meta_to_dto(meta: &domain::CaseMeta) -> CaseSummaryDto {
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
pub fn create_case(
    state: State<AppState>,
    request: CreateCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    let root = PathBuf::from(&request.case_root);
    let active = case_service::create_case(&root, &request.name, request.examiner.as_deref())
        .map_err(CommandError::from_service_error)?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    *guard = Some(active);
    remember_recent_case(&root, &dto)?;
    Ok(dto)
}

#[tauri::command]
pub fn open_case(
    state: State<AppState>,
    request: OpenCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    let root = PathBuf::from(&request.case_root);
    let active = case_service::open_case(&root).map_err(CommandError::from_service_error)?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    *guard = Some(active);
    remember_recent_case(&root, &dto)?;
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

#[tauri::command]
pub fn close_case(state: State<AppState>) -> Result<(), CommandError> {
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    *guard = None;
    Ok(())
}

#[tauri::command]
pub async fn get_case_metrics(state: State<'_, AppState>) -> Result<CaseMetricsDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => {
                    return Ok(CaseMetricsDto {
                        data_source_count: 0,
                        indexed_file_count: 0,
                        timeline_event_count: 0,
                        artifact_count: 0,
                    })
                }
            }
        };
        // Guard is now dropped — query with a fresh connection
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        let repo = persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn);
        let metrics = repo
            .get_metrics()
            .map_err(CommandError::from_service_error)?;
        Ok(CaseMetricsDto {
            data_source_count: metrics.data_source_count,
            indexed_file_count: metrics.indexed_file_count,
            timeline_event_count: metrics.timeline_event_count,
            artifact_count: metrics.artifact_count,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_recent_objects(
    state: State<'_, AppState>,
) -> Result<Vec<RecentObjectDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::file_service::get_recent_objects_real(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_data_sources(
    state: State<'_, AppState>,
) -> Result<Vec<DataSourceSummaryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (db_path, case_id) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => (active.db_path(), active.meta.id.clone()),
                None => return Ok(vec![]),
            }
        };
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::file_service::get_data_sources_real(&conn, &case_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn rename_data_source(
    state: State<'_, AppState>,
    request: RenameDataSourceRequest,
) -> Result<(), CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        let Some(_active) = guard.as_ref() else {
            return Err(CommandError::no_active_case());
        };
        _active
            .with_conn(|conn| {
                app_services::file_service::rename_data_source_real(
                    conn,
                    &request.data_source_id,
                    &request.name,
                )
                .map_err(persistence_sqlite::DbError::System)
            })
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub fn get_recent_cases() -> Result<Vec<RecentCaseDto>, CommandError> {
    read_recent_cases()
}

#[tauri::command]
pub fn remove_case_from_list(request: DeleteCaseRequest) -> Result<String, CommandError> {
    let mut recent = read_recent_cases().unwrap_or_else(|e| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", e);
        Vec::new()
    });
    recent.retain(|item| item.case_root != request.case_root);
    save_recent_cases(&recent)?;
    Ok(format!("Removed from list: {}", request.case_root))
}

#[tauri::command]
pub async fn delete_case(
    state: State<'_, AppState>,
    request: DeleteCaseRequest,
) -> Result<String, CommandError> {
    let root = PathBuf::from(&request.case_root);

    {
        let mut guard = state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        if let Some(ref active) = *guard {
            if active.case_root == root {
                *guard = None;
            }
        }
    }

    let root_clone = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        case_service::delete_case(&root_clone).map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;

    let mut recent = read_recent_cases().unwrap_or_else(|e| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", e);
        Vec::new()
    });
    recent.retain(|item| item.case_root != request.case_root);
    save_recent_cases(&recent)?;

    Ok("Case deleted".to_string())
}

#[tauri::command]
pub async fn delete_data_source(
    state: State<'_, AppState>,
    request: DeleteDataSourceRequest,
) -> Result<String, CommandError> {
    let app_state = state.inner().clone();
    let ds_id = request.data_source_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        let Some(active) = guard.as_ref() else {
            return Err(CommandError::no_active_case());
        };
        active
            .with_conn(|conn| {
                case_service::delete_data_source(conn, &ds_id)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))
            })
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;
    Ok("Data source deleted".to_string())
}

const RECENT_CASES_FILE: &str = "forensics-recent-cases.json";
const MAX_RECENT_CASES: usize = 8;

fn recent_cases_path() -> Result<PathBuf, CommandError> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .ok_or_else(|| CommandError::internal("Cannot resolve APPDATA for recent cases"))?;
    Ok(base.join("ForensicsWorkbench").join(RECENT_CASES_FILE))
}

fn save_recent_cases(recent: &[RecentCaseDto]) -> Result<(), CommandError> {
    let path = recent_cases_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            tracing::error!("Failed to create recent cases directory: {}", e);
            CommandError::internal("Failed to save recent cases")
        })?;
    }
    let json = serde_json::to_string_pretty(recent).map_err(|e| {
        tracing::error!("Failed to serialize recent cases: {}", e);
        CommandError::internal("Failed to save recent cases")
    })?;
    std::fs::write(&path, json).map_err(|e| {
        tracing::error!("Failed to write recent cases file: {}", e);
        CommandError::internal("Failed to save recent cases")
    })?;

    // Set restrictive permissions (Windows: current user only)
    #[cfg(target_os = "windows")]
    {
        // On Windows, we rely on NTFS permissions inherited from %APPDATA%
        // which is already user-specific. Log if we can't verify.
        tracing::debug!("Recent cases file saved to user-specific APPDATA directory");
    }

    // Set restrictive permissions (Unix: 0o600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!("Failed to set restrictive permissions on recent cases file: {}", e);
        }
    }

    Ok(())
}

fn remember_recent_case(
    case_root: &std::path::Path,
    summary: &CaseSummaryDto,
) -> Result<(), CommandError> {
    let mut recent = read_recent_cases().unwrap_or_else(|e| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", e);
        Vec::new()
    });
    recent.retain(|item| item.case_root != case_root.display().to_string());
    recent.insert(
        0,
        RecentCaseDto {
            case_root: case_root.display().to_string(),
            name: summary.name.clone(),
            opened_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    recent.truncate(MAX_RECENT_CASES);
    save_recent_cases(&recent)
}

fn read_recent_cases() -> Result<Vec<RecentCaseDto>, CommandError> {
    let path = recent_cases_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path).map_err(|e| {
        tracing::error!("Failed to read recent cases file: {}", e);
        CommandError::internal("Failed to read recent cases")
    })?;
    serde_json::from_str(&content).map_err(|e| {
        tracing::error!("Failed to parse recent cases JSON: {}", e);
        CommandError::internal("Failed to read recent cases")
    })
}
