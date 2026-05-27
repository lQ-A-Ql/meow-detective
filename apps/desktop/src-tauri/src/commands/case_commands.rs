use app_services::case_service;
use persistence_sqlite::DbError;
use std::path::PathBuf;
use tauri::State;
use transport::{
    commands::RenameDataSourceRequest,
    dto::{CaseMetricsDto, CaseSummaryDto, DataSourceSummaryDto, RecentCaseDto, RecentObjectDto},
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
    case_root: String,
    name: String,
    examiner: Option<String>,
) -> Result<CaseSummaryDto, String> {
    let root = PathBuf::from(&case_root);
    let active =
        case_service::create_case(&root, &name, examiner.as_deref()).map_err(|e| e.to_string())?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state.active_case.lock().map_err(|e| e.to_string())?;
    *guard = Some(active);
    remember_recent_case(&root, &dto)?;
    Ok(dto)
}

#[tauri::command]
pub fn open_case(state: State<AppState>, case_root: String) -> Result<CaseSummaryDto, String> {
    let root = PathBuf::from(&case_root);
    let active = case_service::open_case(&root).map_err(|e| e.to_string())?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state.active_case.lock().map_err(|e| e.to_string())?;
    *guard = Some(active);
    remember_recent_case(&root, &dto)?;
    Ok(dto)
}

#[tauri::command]
pub fn get_current_case(state: State<AppState>) -> Result<Option<CaseSummaryDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    match guard.as_ref() {
        Some(active) => Ok(Some(meta_to_dto(&active.meta))),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn close_case(state: State<AppState>) -> Result<(), String> {
    let mut guard = state.active_case.lock().map_err(|e| e.to_string())?;
    *guard = None;
    Ok(())
}

#[tauri::command]
pub fn get_case_metrics(state: State<AppState>) -> Result<CaseMetricsDto, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if let Some(active) = guard.as_ref() {
        let metrics = active
            .with_conn(|conn| {
                let file_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM file_entries", [], |r| r.get(0))
                    .unwrap_or(0);
                let artifact_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get(0))
                    .unwrap_or(0);
                let timeline_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r.get(0))
                    .unwrap_or(0);
                let ds_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM data_sources", [], |r| r.get(0))
                    .unwrap_or(0);
                Ok(CaseMetricsDto {
                    data_source_count: ds_count as u64,
                    indexed_file_count: file_count as u64,
                    timeline_event_count: timeline_count as u64,
                    artifact_count: artifact_count as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(metrics)
    } else {
        Ok(CaseMetricsDto {
            data_source_count: 0,
            indexed_file_count: 0,
            timeline_event_count: 0,
            artifact_count: 0,
        })
    }
}

#[tauri::command]
pub fn get_recent_objects(state: State<AppState>) -> Result<Vec<RecentObjectDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let Some(active) = guard.as_ref() else {
        return Ok(vec![]);
    };
    active
        .with_conn(|conn| {
            app_services::file_service::get_recent_objects_real(conn).map_err(DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_data_sources(state: State<AppState>) -> Result<Vec<DataSourceSummaryDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let Some(active) = guard.as_ref() else {
        return Ok(vec![]);
    };
    active
        .with_conn(|conn| {
            app_services::file_service::get_data_sources_real(conn, &active.meta.id)
                .map_err(DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_data_source(
    state: State<AppState>,
    request: RenameDataSourceRequest,
) -> Result<(), String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let Some(active) = guard.as_ref() else {
        return Err("No active case".to_string());
    };
    active
        .with_conn(|conn| {
            app_services::file_service::rename_data_source_real(
                conn,
                &request.data_source_id,
                &request.name,
            )
            .map_err(DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recent_cases() -> Result<Vec<RecentCaseDto>, String> {
    read_recent_cases()
}

const RECENT_CASES_FILE: &str = "forensics-recent-cases.json";
const MAX_RECENT_CASES: usize = 8;

fn recent_cases_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .ok_or("Cannot resolve APPDATA for recent cases".to_string())?;
    Ok(base.join("ForensicsWorkbench").join(RECENT_CASES_FILE))
}

fn remember_recent_case(
    case_root: &std::path::Path,
    summary: &CaseSummaryDto,
) -> Result<(), String> {
    let mut recent = read_recent_cases().unwrap_or_default();
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

    let path = recent_cases_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&recent).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn read_recent_cases() -> Result<Vec<RecentCaseDto>, String> {
    let path = recent_cases_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}
