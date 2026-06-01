//! Data source analysis commands.

use app_services::analysis_service::{self, FileClassification, SystemInfo};
use tauri::State;
use transport::CommandError;

use crate::state::AppState;

/// Get system information from the current case.
#[tauri::command]
pub async fn get_system_info(state: State<'_, AppState>) -> Result<SystemInfo, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;

        let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
        let case_root = active.case_root.clone();

        Ok(analysis_service::extract_system_info(&case_root))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Classify files by magic bytes.
#[tauri::command]
pub async fn classify_files(
    state: State<'_, AppState>,
    sample_size: Option<usize>,
) -> Result<Vec<FileClassification>, CommandError> {
    let app_state = state.inner().clone();
    let sample = sample_size.unwrap_or(1000);

    tauri::async_runtime::spawn_blocking(move || {
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(Vec::new()),
            }
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        // Get all files from database
        let file_repo = persistence_sqlite::repositories::file_repo::FileRepo::new(&conn);
        let roots = file_repo
            .find_root_entries()
            .map_err(CommandError::from_service_error)?;

        let mut all_files = Vec::new();
        let mut queue: Vec<domain::FileEntry> = roots;

        while let Some(entry) = queue.pop() {
            if entry.entry_type == domain::EntryType::Directory {
                let children = file_repo
                    .find_children(&entry.id)
                    .map_err(CommandError::from_service_error)?;
                queue.extend(children);
            } else {
                all_files.push((entry.path.clone(), entry.size.unwrap_or(0)));
            }
        }

        // Get data source path for file reading
        let case_root = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            guard
                .as_ref()
                .map(|a| a.case_root.clone())
                .unwrap_or_default()
        };

        // Classify files
        let classifications = analysis_service::classify_files_by_magic(
            &all_files,
            sample,
            |path| {
                let full_path = case_root.join(path);
                std::fs::read(&full_path).ok()
            },
        );

        Ok(classifications)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Generate analysis summary report.
#[tauri::command]
pub async fn generate_analysis_summary(
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;

        let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
        let case_root = active.case_root.clone();

        let system_info = analysis_service::extract_system_info(&case_root);

        // Get classifications
        let db_path = active.db_path();
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        let file_repo = persistence_sqlite::repositories::file_repo::FileRepo::new(&conn);
        let roots = file_repo
            .find_root_entries()
            .map_err(CommandError::from_service_error)?;

        let mut all_files = Vec::new();
        let mut queue: Vec<domain::FileEntry> = roots;

        while let Some(entry) = queue.pop() {
            if entry.entry_type == domain::EntryType::Directory {
                let children = file_repo
                    .find_children(&entry.id)
                    .map_err(CommandError::from_service_error)?;
                queue.extend(children);
            } else {
                all_files.push((entry.path.clone(), entry.size.unwrap_or(0)));
            }
        }

        let classifications = analysis_service::classify_files_by_magic(
            &all_files,
            1000,
            |path| {
                let full_path = case_root.join(path);
                std::fs::read(&full_path).ok()
            },
        );

        Ok(analysis_service::generate_analysis_summary(
            &system_info,
            &classifications,
        ))
    })
    .await
    .map_err(CommandError::from_join_error)?
}
