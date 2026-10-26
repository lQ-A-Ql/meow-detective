//! Re-exports the command identifiers needed by the desktop registration macro.

pub(crate) use crate::commands::{
    analysis_commands::{
        classify_files, generate_analysis_summary, get_android_device_info,
        get_android_package_summary, get_browser_history_summary, get_case_overview_snapshot,
        get_correlation_snapshot, get_email_extraction_summary,
        get_evidence_classification_summary, get_evtx_event_summary, get_file_classification_board,
        get_kubernetes_cluster_summary, get_linux_artifact_summary, get_linux_evidence_events,
        get_linux_evidence_set_summary, get_plugin_family_entries, get_registry_extraction_summary,
        get_registry_structured_summary, get_system_info, get_v2_governance_snapshot,
        get_v3_governance_snapshot, list_linux_evidence_sets, list_plugin_modules,
        run_analysis_extraction, run_android_analysis, run_evidence_classification,
        run_kubernetes_cluster_analysis,
    },
    app_update_commands::{check_app_update, get_app_update_info, install_app_update},
    artifact_commands::{
        get_artifact_by_id, get_artifact_families, get_artifact_family_counts, get_artifact_rows,
        get_artifact_rows_request,
    },
    batch_commands::{
        cancel_batch, create_batch_plan, get_batch_job, list_batch_jobs, pause_batch, resume_batch,
        start_batch,
    },
    carving_commands::{
        export_carved_artifact, read_carved_artifact_range, search_carved_artifacts,
    },
    case_commands::{
        close_case, create_case, delete_case, delete_data_source, get_case_metrics,
        get_current_case, get_data_sources, get_nested_evidence_lineage, get_recent_cases,
        get_recent_objects, materialize_nested_evidence, open_case, remove_case_from_list,
        rename_data_source,
    },
    digest_commands::{calculate_evidence_digest, list_evidence_digests},
    emulation_commands::{
        apply_emulation_bypass, apply_emulation_linux_bypass, cleanup_emulation_osdata,
        get_emulation_bypass_accounts, get_emulation_linux_accounts, get_emulation_preflight,
        get_emulation_status, install_emulation_efi_fallback, launch_emulation,
        list_emulation_sessions, prepare_emulation, release_emulation,
        repair_emulation_fs_journals,
    },
    evidence_task_commands::{
        cancel_evidence_task, get_content_index_coverage, get_evidence_task, start_evidence_task,
    },
    file_commands::{
        browse_registry_key, cancel_bitlocker_dictionary_attack, close_file_handle,
        export_deleted_recovery, extract_file, forget_persisted_bitlocker_key,
        get_bitlocker_dictionary_attack_status, get_document_preview, get_file_children_request,
        get_file_jump_context, get_file_rows_request, get_file_tree_request, get_image_metadata,
        get_image_preview, get_media_url, get_text_preview, import_unlocked_bitlocker_catalog,
        inspect_bitlocker_volume, inspect_ntfs_file, list_deleted_recoveries, list_local_disks,
        lock_bitlocker_volume, open_file_handle_request, read_deleted_recovery_range,
        read_file_range, read_media_range, restore_persisted_bitlocker_key, run_deleted_recovery,
        search_deleted_recoveries_by_hash, start_bitlocker_dictionary_attack,
        unlock_bitlocker_with_memory_image, unlock_bitlocker_with_password,
        unlock_bitlocker_with_recovery_password,
    },
    graph_commands::{
        get_graph_snapshot, get_node_neighborhood, get_provenance_chain, list_graph_nodes,
        query_graph,
    },
    import::pipeline::{cancel_import, import_data_source},
    infrastructure_commands::get_infrastructure_graph,
    job_commands::{get_jobs_snapshot, get_trace_items, get_warnings},
    ledger_commands::{get_ledger_proof, get_ledger_snapshot, seal_ledger_batch},
    mcp_commands::{
        call_mcp_host_tool, create_mcp_host_client, get_mcp_host_status, list_mcp_host_clients,
        list_mcp_host_tools, revoke_mcp_host_client, set_mcp_host_settings,
    },
    mount_commands::{
        get_mount_status, list_mounts, mount_image, mount_physical_image, unmount_image,
    },
    notebook_commands::{
        add_evidence_citation, create_notebook_entry, get_notebook_thread,
        list_investigation_steps, list_notebook_entries, update_notebook_entry,
    },
    plugin_action_commands::{list_plugin_actions, recover_wechat_keys},
    registry_browser_commands::{
        close_registry_hive, get_registry_hardware, list_registry_hives, list_registry_subkeys,
        list_registry_values, locate_registry_search_hit, open_registry_hive,
        read_registry_value_range, resolve_registry_key, search_registry_hive,
    },
    report_commands::{
        export_csv_correlation_report, export_csv_report, export_html_report, export_json_report,
        get_report_history, get_report_templates,
    },
    rule_pack_commands::{list_loaded_rule_packs, load_rule_pack, validate_rule_pack},
    search_commands::search_files_request,
    settings_commands::{get_app_settings, save_app_settings},
    timeline_commands::{get_timeline_event_by_id, get_timeline_events, get_timeline_facets},
};
