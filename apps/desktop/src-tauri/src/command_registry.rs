//! Re-exports the command identifiers needed by the desktop registration macro.

pub(crate) use crate::commands::{
    analysis_commands::{
        classify_files, generate_analysis_summary, get_browser_history_summary,
        get_case_overview_snapshot, get_correlation_snapshot, get_email_extraction_summary,
        get_evidence_classification_summary, get_evtx_event_summary, get_file_classification_board,
        get_linux_artifact_summary, get_registry_extraction_summary,
        get_registry_structured_summary, get_system_info, get_v2_governance_snapshot,
        get_v3_governance_snapshot, run_analysis_extraction, run_evidence_classification,
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
    emulation_commands::{
        get_emulation_preflight, get_emulation_status, launch_emulation, list_emulation_sessions,
        prepare_emulation, release_emulation,
    },
    file_commands::{
        close_file_handle, export_deleted_recovery, extract_file, forget_persisted_bitlocker_key,
        get_document_preview, get_file_children, get_file_children_request, get_file_jump_context,
        get_file_rows, get_file_rows_request, get_file_tree, get_file_tree_request,
        get_image_preview, get_media_url, get_text_preview, import_unlocked_bitlocker_catalog,
        inspect_bitlocker_volume, list_deleted_recoveries, lock_bitlocker_volume, open_file_handle,
        open_file_handle_request, read_deleted_recovery_range, read_file_range, read_media_range,
        restore_persisted_bitlocker_key, run_deleted_recovery, unlock_bitlocker_with_memory_image,
        unlock_bitlocker_with_password, unlock_bitlocker_with_recovery_password,
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
    mount_commands::{
        get_mount_status, list_mounts, mount_image, mount_physical_image, unmount_image,
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
    timeline_commands::{get_timeline_event_by_id, get_timeline_events, get_timeline_facets},
};
