use app_services::case_service;
use std::path::PathBuf;
use tauri::State;
use transport::dto::{CaseMetricsDto, CaseSummaryDto, RecentObjectDto};

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
    Ok(dto)
}

#[tauri::command]
pub fn open_case(state: State<AppState>, case_root: String) -> Result<CaseSummaryDto, String> {
    let root = PathBuf::from(&case_root);
    let active = case_service::open_case(&root).map_err(|e| e.to_string())?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state.active_case.lock().map_err(|e| e.to_string())?;
    *guard = Some(active);
    Ok(dto)
}

#[tauri::command]
pub fn get_current_case(state: State<AppState>) -> Result<CaseSummaryDto, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    match guard.as_ref() {
        Some(active) => Ok(meta_to_dto(&active.meta)),
        None => Err("No active case".into()),
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
    let _active = guard.as_ref().ok_or("No active case")?;
    // TODO: query recent objects from DB
    Ok(vec![])
}
