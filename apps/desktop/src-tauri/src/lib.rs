mod commands;
pub mod events;
pub mod state;

use commands::{
    artifact_commands::{get_artifact_families, get_artifact_rows, get_artifact_rows_request},
    case_commands::{close_case, create_case, get_case_metrics, get_current_case, get_recent_objects, open_case},
    file_commands::{get_file_rows, get_file_tree, import_data_source, open_file_handle, open_file_handle_request, read_file_range},
    job_commands::{get_jobs_snapshot, get_trace_items, get_warnings},
    report_commands::{get_report_history, get_report_templates},
    search_commands::{search_files, search_files_request},
    timeline_commands::get_timeline_events,
};
use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            create_case,
            open_case,
            close_case,
            get_current_case,
            get_case_metrics,
            get_recent_objects,
            import_data_source,
            get_file_tree,
            get_file_rows,
            open_file_handle,
            open_file_handle_request,
            read_file_range,
            search_files,
            search_files_request,
            get_timeline_events,
            get_artifact_families,
            get_artifact_rows,
            get_artifact_rows_request,
            get_report_templates,
            get_report_history,
            get_jobs_snapshot,
            get_warnings,
            get_trace_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
