mod commands;
pub mod events;
pub mod state;

use commands::{
    analysis_commands::{classify_files, generate_analysis_summary, get_system_info},
    artifact_commands::{get_artifact_families, get_artifact_rows, get_artifact_rows_request},
    case_commands::{
        close_case, create_case, delete_case, delete_data_source, get_case_metrics,
        get_current_case, get_data_sources, get_recent_cases, get_recent_objects, open_case,
        remove_case_from_list, rename_data_source,
    },
    file_commands::{
        get_file_children, get_file_children_request, get_file_rows, get_file_rows_request,
        get_file_tree, get_image_preview, get_media_url, get_text_preview, open_file_handle, open_file_handle_request, read_file_range,
    },
    import::pipeline::{cancel_import, import_data_source},
    job_commands::{get_jobs_snapshot, get_trace_items, get_warnings},
    mcp_commands::{
        add_mcp_server, call_mcp_tool, connect_mcp_server, disconnect_mcp_server,
        get_mcp_config, get_mcp_prompt, list_mcp_prompts, list_mcp_resources, list_mcp_tools,
        remove_mcp_server, save_mcp_config, test_mcp_connection,
    },
    report_commands::{
        export_csv_report, export_html_report, export_json_report, get_report_history,
        get_report_templates,
    },
    search_commands::{search_files, search_files_request},
    timeline_commands::get_timeline_events,
};
use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            create_case,
            open_case,
            close_case,
            get_current_case,
            get_case_metrics,
            get_recent_objects,
            get_recent_cases,
            get_data_sources,
            rename_data_source,
            delete_case,
            delete_data_source,
            remove_case_from_list,
            import_data_source,
            cancel_import,
            get_file_children,
            get_file_children_request,
            get_file_tree,
            get_file_rows,
            get_file_rows_request,
            open_file_handle,
            open_file_handle_request,
            read_file_range,
            get_text_preview,
            get_image_preview,
            get_media_url,
            search_files,
            search_files_request,
            get_timeline_events,
            get_artifact_families,
            get_artifact_rows,
            get_artifact_rows_request,
            get_report_templates,
            get_report_history,
            export_html_report,
            export_csv_report,
            export_json_report,
            get_jobs_snapshot,
            get_system_info,
            classify_files,
            generate_analysis_summary,
            get_warnings,
            get_trace_items,
            // MCP commands
            get_mcp_config,
            save_mcp_config,
            add_mcp_server,
            remove_mcp_server,
            connect_mcp_server,
            disconnect_mcp_server,
            test_mcp_connection,
            list_mcp_resources,
            list_mcp_tools,
            call_mcp_tool,
            list_mcp_prompts,
            get_mcp_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
