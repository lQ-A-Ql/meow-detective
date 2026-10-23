$ErrorActionPreference = "Stop"

$commandRoot = Join-Path $PSScriptRoot "..\apps\desktop\src-tauri\src\commands"
if (-not (Test-Path -LiteralPath $commandRoot)) {
  throw "Command root not found at $commandRoot"
}

# Query commands may use read-only case connections or helpers that obtain one.
# A query command must not acquire the mutable case connection directly.
$policies = [ordered]@{
  "digest_commands.rs" = @("list_evidence_digests")
  "carving_commands.rs" = @("read_carved_artifact_range", "search_carved_artifacts")
  "evidence_task_commands.rs" = @("get_content_index_coverage", "get_evidence_task")
  "case_commands/metrics.rs" = @(
    "get_case_metrics",
    "get_recent_objects",
    "get_data_sources",
    "get_nested_evidence_lineage"
  )
  "file_commands/browse.rs" = @(
    "get_file_children_request",
    "get_file_tree_request",
    "get_file_rows_request",
    "get_file_jump_context"
  )
  "file_commands/recovery.rs" = @(
    "list_deleted_recoveries",
    "search_deleted_recoveries_by_hash",
    "read_deleted_recovery_range"
  )
  "file_commands/viewer.rs" = @(
    "open_file_handle_request",
    "close_file_handle",
    "read_file_range"
  )
  "file_commands/bitlocker.rs" = @("inspect_bitlocker_volume")
  "file_commands/preview.rs" = @(
    "get_text_preview",
    "get_image_preview",
    "get_image_metadata",
    "get_document_preview"
  )
  "file_commands/ntfs.rs" = @("inspect_ntfs_file")
  "file_commands/media.rs" = @("get_media_url", "read_media_range")
  "file_commands/registry.rs" = @("browse_registry_key")
  "registry_browser_commands/discovery.rs" = @("list_registry_hives")
  "registry_browser_commands/opening.rs" = @("open_registry_hive")
  "registry_browser_commands/hardware.rs" = @("get_registry_hardware")
  "registry_browser_commands/resolve.rs" = @("resolve_registry_key")
  "registry_browser_commands/subkeys.rs" = @("list_registry_subkeys")
  "registry_browser_commands/values.rs" = @("list_registry_values")
  "registry_browser_commands/search.rs" = @("search_registry_hive")
  "registry_browser_commands/search_location.rs" = @("locate_registry_search_hit")
  "registry_browser_commands/range.rs" = @("read_registry_value_range")
  "registry_browser_commands/close.rs" = @("close_registry_hive")
  "search_commands.rs" = @("search_files_request")
  "timeline_commands.rs" = @(
    "get_timeline_events",
    "get_timeline_event_by_id",
    "get_timeline_facets"
  )
  "artifact_commands.rs" = @(
    "get_artifact_families",
    "get_artifact_rows",
    "get_artifact_rows_request",
    "get_artifact_family_counts",
    "get_artifact_by_id"
  )
  "batch_commands/query.rs" = @("get_batch_job", "list_batch_jobs")
  "report_commands.rs" = @("get_report_history")
  "job_commands.rs" = @("get_jobs_snapshot", "get_warnings", "get_trace_items")
  "analysis_commands/queries.rs" = @(
    "get_system_info",
    "get_kubernetes_cluster_summary",
    "get_android_device_info",
    "get_android_package_summary",
    "get_evidence_classification_summary"
  )
  "analysis_commands/linux_cluster.rs" = @(
    "get_linux_evidence_set_summary",
    "list_linux_evidence_sets"
  )
  "analysis_commands/linux_events.rs" = @("get_linux_evidence_events")
  "analysis_commands/classification.rs" = @(
    "classify_files",
    "get_file_classification_board"
  )
  "analysis_commands/extraction/summaries.rs" = @(
    "get_registry_extraction_summary",
    "get_registry_structured_summary",
    "get_browser_history_summary",
    "get_email_extraction_summary",
    "get_evtx_event_summary",
    "get_linux_artifact_summary"
  )
  "analysis_commands/extraction/plugin_modules.rs" = @(
    "list_plugin_modules",
    "get_plugin_family_entries"
  )
  "analysis_commands/governance.rs" = @(
    "get_v2_governance_snapshot",
    "get_v3_governance_snapshot",
    "get_case_overview_snapshot",
    "get_correlation_snapshot"
  )
  "analysis_commands/extraction/report.rs" = @("generate_analysis_summary")
  "infrastructure_commands.rs" = @("get_infrastructure_graph")
  "graph_commands.rs" = @(
    "get_graph_snapshot",
    "query_graph",
    "list_graph_nodes",
    "get_node_neighborhood",
    "get_provenance_chain"
  )
  "ledger_commands.rs" = @("get_ledger_snapshot", "get_ledger_proof")
  "notebook_commands.rs" = @(
    "list_notebook_entries",
    "get_notebook_thread",
    "list_investigation_steps"
  )
  "plugin_action_commands.rs" = @("list_plugin_actions")
  "emulation_commands/preflight.rs" = @("get_emulation_preflight")
  "emulation_commands/bypass.rs" = @(
    "get_emulation_bypass_accounts",
    "get_emulation_linux_accounts"
  )
}

$forbiddenPatterns = @(
  'app_services::connection::open_case_db\s*\(',
  'app_services::connection::open_existing_case_db\s*\(',
  '\bget_case_connection\s*\(',
  '\.get_connection\s*\(',
  '\brun_active_case_command\s*\('
)

$violations = @()
foreach ($relativePath in $policies.Keys) {
  $path = Join-Path $commandRoot ($relativePath -replace '/', '\')
  if (-not (Test-Path -LiteralPath $path)) {
    $violations += "${relativePath}: command file not found"
    continue
  }

  $content = (Get-Content -LiteralPath $path -Encoding UTF8) -join "`n"
  $blocks = [regex]::Split($content, '(?m)^\s*#\[tauri::command\]\s*')
  $seen = @{}
  foreach ($block in $blocks) {
    $nameMatch = [regex]::Match($block, '\bpub\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)')
    if (-not $nameMatch.Success) {
      continue
    }
    $name = $nameMatch.Groups[1].Value
    if ($policies[$relativePath] -notcontains $name) {
      continue
    }
    $seen[$name] = $true
    foreach ($pattern in $forbiddenPatterns) {
      if ($block -match $pattern) {
        $violations += "${relativePath}:${name}: query command acquires a mutable case connection"
      }
    }
  }

  foreach ($expected in $policies[$relativePath]) {
    if (-not $seen.ContainsKey($expected)) {
      $violations += "${relativePath}:${expected}: registered query command was not found"
    }
  }
}

if ($violations.Count -gt 0) {
  Write-Error "Query command boundary guard failed:`n$($violations -join "`n")"
}

Write-Host "Query command boundary guard passed"
