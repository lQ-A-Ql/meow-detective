mod cache_invalidation;
mod commands;
pub mod events;
mod media_protocol;
mod platform_security;
pub mod state;

use commands::{
    analysis_commands::{
        classify_files, generate_analysis_summary, get_browser_history_summary,
        get_correlation_snapshot, get_email_extraction_summary,
        get_evidence_classification_summary, get_evtx_event_summary, get_linux_artifact_summary,
        get_registry_extraction_summary, get_registry_structured_summary, get_system_info,
        get_v2_governance_snapshot, get_v3_governance_snapshot, run_analysis_extraction,
        run_evidence_classification,
    },
    artifact_commands::{
        get_artifact_by_id, get_artifact_families, get_artifact_family_counts, get_artifact_rows,
        get_artifact_rows_request,
    },
    batch_commands::{
        cancel_batch, create_batch_plan, get_batch_job, list_batch_jobs, pause_batch, resume_batch,
        start_batch,
    },
    case_commands::{
        close_case, create_analysis_demo_case, create_case, delete_case, delete_data_source,
        get_case_metrics, get_current_case, get_data_sources, get_recent_cases, get_recent_objects,
        open_case, remove_case_from_list, rename_data_source,
    },
    file_commands::{
        extract_file, get_file_children, get_file_children_request, get_file_jump_context,
        get_file_rows, get_file_rows_request, get_file_tree, get_file_tree_request,
        get_image_preview, get_media_url, get_text_preview, open_file_handle,
        open_file_handle_request, read_file_range, read_media_range,
    },
    graph_commands::{
        get_graph_snapshot, get_node_neighborhood, get_provenance_chain, list_graph_nodes,
        query_graph,
    },
    import::pipeline::{cancel_import, import_data_source},
    job_commands::{get_jobs_snapshot, get_trace_items, get_warnings},
    mcp_commands::{
        add_mcp_server, call_mcp_tool, connect_mcp_server, disconnect_mcp_server, get_mcp_config,
        get_mcp_prompt, list_mcp_prompts, list_mcp_resources, list_mcp_tools, remove_mcp_server,
        save_mcp_config, test_mcp_connection,
    },
    notebook_commands::{
        add_evidence_citation, create_notebook_entry, get_notebook_thread,
        list_investigation_steps, list_notebook_entries, update_notebook_entry,
    },
    report_commands::{
        export_csv_correlation_report, export_csv_report, export_html_report, export_json_report,
        get_report_history, get_report_templates,
    },
    rule_pack_commands::{list_loaded_rule_packs, load_rule_pack, validate_rule_pack},
    search_commands::{search_files, search_files_request},
    settings_commands::{get_app_settings, save_app_settings},
    timeline_commands::{get_timeline_event_by_id, get_timeline_events},
};
use state::AppState;

pub fn run() {
    let builder = tauri::Builder::default();
    match media_protocol::register(builder)
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            cache_invalidation::register(app.handle().clone());
            Ok(())
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            create_case,
            create_analysis_demo_case,
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
            get_file_tree_request,
            get_file_rows,
            get_file_rows_request,
            get_file_jump_context,
            open_file_handle,
            open_file_handle_request,
            read_file_range,
            extract_file,
            get_text_preview,
            get_image_preview,
            get_media_url,
            read_media_range,
            search_files,
            search_files_request,
            get_app_settings,
            save_app_settings,
            get_timeline_events,
            get_timeline_event_by_id,
            get_artifact_families,
            get_artifact_rows,
            get_artifact_rows_request,
            get_artifact_family_counts,
            get_artifact_by_id,
            create_batch_plan,
            start_batch,
            pause_batch,
            resume_batch,
            cancel_batch,
            get_batch_job,
            list_batch_jobs,
            get_report_templates,
            get_report_history,
            export_html_report,
            export_csv_correlation_report,
            export_csv_report,
            export_json_report,
            get_jobs_snapshot,
            get_system_info,
            classify_files,
            get_evidence_classification_summary,
            run_evidence_classification,
            run_analysis_extraction,
            get_registry_extraction_summary,
            get_registry_structured_summary,
            get_browser_history_summary,
            get_email_extraction_summary,
            get_evtx_event_summary,
            get_linux_artifact_summary,
            get_v2_governance_snapshot,
            get_v3_governance_snapshot,
            get_correlation_snapshot,
            generate_analysis_summary,
            get_graph_snapshot,
            query_graph,
            list_graph_nodes,
            get_node_neighborhood,
            get_provenance_chain,
            get_warnings,
            get_trace_items,
            // Rule pack commands
            list_loaded_rule_packs,
            load_rule_pack,
            validate_rule_pack,
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
            // Notebook commands
            create_notebook_entry,
            update_notebook_entry,
            list_notebook_entries,
            get_notebook_thread,
            add_evidence_citation,
            list_investigation_steps,
        ])
        .run(tauri::generate_context!())
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("Failed to run Tauri application: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
