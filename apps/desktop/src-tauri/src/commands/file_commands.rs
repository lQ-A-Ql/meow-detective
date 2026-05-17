use app_services::{datasource_service, file_service, search_service, timeline_service};
use domain::DataSourceKind;
use evidence_core::{probe, LogicalFsReader};
use std::path::PathBuf;
use tauri::State;
use transport::dto::FileTreeNodeDto;

use crate::state::AppState;

fn run_post_import_pipeline(
    conn: &rusqlite::Connection,
    ds_id: &domain::DataSourceId,
    index_dir: &std::path::Path,
) -> persistence_sqlite::DbResult<String> {
    let file_repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);

    // Read all files for timeline projection
    let roots = file_repo.find_roots(ds_id)?;
    let mut all_files = Vec::new();
    let mut queue = roots;
    while let Some(f) = queue.pop() {
        if f.entry_type != domain::EntryType::Directory {
            all_files.push(f);
        } else {
            let children = file_repo.find_children(&f.id)?;
            queue.extend(children);
        }
    }

    // Timeline projection
    let tl_count = timeline_service::project_and_store_macb(conn, &all_files)
        .map_err(persistence_sqlite::DbError::System)?;

    // Text indexing (first 1000 files only, MVP)
    let to_index: Vec<domain::FileEntryId> = all_files.iter().take(1000).map(|f| f.id.clone()).collect();
    let index_result = search_service::index_files(
        conn,
        index_dir,
        &to_index,
        |_file_id| -> Option<Box<dyn std::io::Read>> { None },
    );

    let index_msg = match index_result {
        Ok(stats) => format!("{} indexed", stats.indexed_count),
        Err(e) => format!("index error: {}", e),
    };

    Ok(format!("Timeline: {} events. Index: {}", tl_count, index_msg))
}

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
    let index_dir = active.case_root.join("indexes").join("tantivy");

    let result_msg = active.with_conn(|conn| {
        let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
        let job_id = job_repo.create(&case_id.0, "import_data_source")?;
        job_repo.update_progress(&job_id, 10, "Attaching data source...")?;
        let ds = datasource_service::attach_data_source(conn, &case_id, &source_name, &path, kind)
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
        job_repo.update_progress(&job_id, 20, "Enumerating filesystem...")?;
        let fs = LogicalFsReader::open(&path, &ds.name)
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
        let stats = file_service::enumerate_filesystem(conn, &ds.id, &fs)?;
        job_repo.update_progress(&job_id, 50, "Projecting timeline...")?;
        let mut msg = format!("Imported: {} files, {} dirs, {} bytes. ", stats.file_count, stats.dir_count, stats.total_size);
        if !stats.warnings.is_empty() {
            msg.push_str(&format!("Warnings: {}. ", stats.warnings.join("; ")));
        }
        job_repo.update_progress(&job_id, 70, "Indexing and pipeline...")?;
        let pipeline_msg = run_post_import_pipeline(conn, &ds.id, &index_dir)?;
        msg.push_str(&pipeline_msg);
        job_repo.complete(&job_id, "Import complete")?;
        Ok(msg)
    }).map_err(|e| e.to_string())?;

    Ok(result_msg)
}

#[tauri::command]
pub fn get_file_children(state: State<AppState>, parent_id: String) -> Result<Vec<FileTreeNodeDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if let Some(active) = guard.as_ref() {
        let items = active.with_conn(|conn| {
            let repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
            let children = repo.find_children(&domain::FileEntryId(parent_id))?;
            let nodes: Vec<FileTreeNodeDto> = children.iter().map(|entry| FileTreeNodeDto {
                id: entry.id.0.clone(),
                name: entry.name.clone(),
                depth: 0,
                expanded: Some(false),
                active: Some(false),
            }).collect();
            Ok(nodes)
        }).map_err(|e| e.to_string())?;
        return Ok(items);
    }
    Ok(vec![])
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
