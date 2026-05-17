use app_services::{datasource_service, file_service};
use domain::DataSourceKind;
use evidence_core::{probe, LogicalFsReader};
use std::path::PathBuf;
use tauri::State;
use transport::dto::FileTreeNodeDto;

use crate::state::AppState;

#[tauri::command]
pub fn import_data_source(
    state: State<AppState>,
    source_path: String,
) -> Result<String, String> {
    let path = PathBuf::from(&source_path);
    let probe_result = probe::probe(&path).map_err(|e| e.to_string())?;

    let kind = if probe_result.candidates.contains(&"logical_directory".to_string()) {
        DataSourceKind::LogicalDirectory
    } else {
        DataSourceKind::Raw
    };

    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;

    let source_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string());

    let case_id = active.meta.id.clone();

    let result_msg = active.with_conn(|conn| {
        let ds = datasource_service::attach_data_source(conn, &case_id, &source_name, &path, kind)
            .map_err(|e| persistence_sqlite::DbError::Migration(e.to_string()))?;
        let fs = LogicalFsReader::open(&path, &ds.name)
            .map_err(|e| persistence_sqlite::DbError::Migration(e.to_string()))?;
        let stats = file_service::enumerate_filesystem(conn, &ds.id, &fs)?;
        let mut msg = format!("Imported: {} files, {} dirs, {} bytes", stats.file_count, stats.dir_count, stats.total_size);
        if !stats.warnings.is_empty() {
            msg.push_str(&format!("\nWarnings: {}", stats.warnings.join("; ")));
        }
        Ok(msg)
    }).map_err(|e| e.to_string())?;

    Ok(result_msg)
}

#[tauri::command]
pub fn get_file_tree(state: State<AppState>) -> Result<Vec<FileTreeNodeDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if let Some(active) = guard.as_ref() {
        let items = active.with_conn(|conn| {
            let repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
            let ds_id: String = conn.query_row(
                "SELECT id FROM data_sources LIMIT 1", [], |r| r.get(0),
            )?;
            let roots = repo.find_roots(&domain::DataSourceId(ds_id))?;
            let nodes: Vec<FileTreeNodeDto> = roots.iter().map(|entry| FileTreeNodeDto {
                id: entry.id.0.clone(),
                name: entry.name.clone(),
                depth: 0,
                expanded: Some(true),
                active: Some(false),
            }).collect();
            Ok(nodes)
        }).map_err(|e| e.to_string())?;
        if !items.is_empty() {
            return Ok(items);
        }
    }
    Ok(app_services::file_service::get_file_tree())
}

#[tauri::command]
pub fn get_file_rows() -> Result<Vec<transport::dto::FileEntryRowDto>, String> {
    Ok(app_services::file_service::get_file_rows())
}

#[tauri::command]
pub fn open_file_handle(file_id: String) -> Result<transport::dto::ViewerHandleDto, String> {
    Ok(app_services::file_service::open_file_handle(file_id))
}

#[tauri::command]
pub fn open_file_handle_request(request: transport::commands::OpenFileHandleRequest) -> Result<transport::dto::ViewerHandleDto, String> {
    Ok(app_services::file_service::open_file_handle(request.file_id))
}

#[tauri::command]
pub fn read_file_range(request: transport::dto::ViewerRangeRequestDto) -> Result<transport::dto::ViewerRangeResponseDto, String> {
    Ok(app_services::file_service::read_file_range(request))
}
