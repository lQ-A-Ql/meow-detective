mod bitlocker_key_store;
mod cache_invalidation;
mod command_registry;
mod commands;
#[cfg(windows)]
mod dokan_runtime;
mod emulation_backend;
mod emulation_registry;
pub mod events;
mod media_protocol;
mod mount_backend;
mod mount_registry;
mod physical_mount_registry;
mod platform_security;
pub mod state;

use command_registry::*;

macro_rules! desktop_command_handler {
    () => {
        tauri::generate_handler![
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
            list_deleted_recoveries,
            search_deleted_recoveries_by_hash,
            run_deleted_recovery,
            read_deleted_recovery_range,
            export_deleted_recovery,
            open_file_handle,
            open_file_handle_request,
            close_file_handle,
            read_file_range,
            inspect_bitlocker_volume,
            unlock_bitlocker_with_password,
            unlock_bitlocker_with_recovery_password,
            unlock_bitlocker_with_memory_image,
            import_unlocked_bitlocker_catalog,
            lock_bitlocker_volume,
            restore_persisted_bitlocker_key,
            forget_persisted_bitlocker_key,
            extract_file,
            get_text_preview,
            get_image_preview,
            get_document_preview,
            get_media_url,
            read_media_range,
            search_files,
            search_files_request,
            get_app_settings,
            save_app_settings,
            get_timeline_events,
            get_timeline_event_by_id,
            get_timeline_facets,
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
            get_file_classification_board,
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
            get_case_overview_snapshot,
            get_correlation_snapshot,
            generate_analysis_summary,
            get_graph_snapshot,
            query_graph,
            list_graph_nodes,
            get_node_neighborhood,
            get_provenance_chain,
            get_warnings,
            get_trace_items,
            list_loaded_rule_packs,
            load_rule_pack,
            validate_rule_pack,
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
            create_notebook_entry,
            update_notebook_entry,
            list_notebook_entries,
            get_notebook_thread,
            add_evidence_citation,
            list_investigation_steps,
            mount_image,
            mount_physical_image,
            unmount_image,
            get_mount_status,
            list_mounts,
            prepare_emulation,
            launch_emulation,
            get_emulation_preflight,
            get_emulation_bypass_accounts,
            apply_emulation_bypass,
            cleanup_emulation_osdata,
            get_emulation_status,
            list_emulation_sessions,
            release_emulation,
        ]
    };
}

pub fn run() {
    desktop_builder()
        .invoke_handler(desktop_command_handler!())
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            tracing::error!("Failed to run Tauri application: {error}");
            std::process::exit(1);
        });
}

fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    media_protocol::register(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            cache_invalidation::register(app.handle().clone());
            Ok(())
        })
        .manage(state::AppState::default())
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
