param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$mcpCommandsPath = Join-Path $repoRoot "apps/desktop/src-tauri/src/commands/mcp_commands.rs"
$mcpDtoPath = Join-Path $repoRoot "crates/transport/src/dto/mcp.rs"
$stagingDir = Join-Path $repoRoot "crates/app-services/src/staging"
$importPipelineDir = Join-Path $repoRoot "crates/app-services/src/import_pipeline"
$importPipelineModPath = Join-Path $importPipelineDir "mod.rs"
$importPipelineEmitPath = Join-Path $importPipelineDir "emit.rs"
$analysisExtractionModPath = Join-Path $repoRoot "crates/app-services/src/analysis_service/extraction/mod.rs"
$filePreviewPath = Join-Path $repoRoot "crates/app-services/src/file_service/preview.rs"
$fileCommandsPath = Join-Path $repoRoot "apps/desktop/src-tauri/src/commands/file_commands.rs"
$datasourceFacadePath = Join-Path $repoRoot "crates/app-services/src/datasource_service.rs"
$linuxIntegrationTestPath = Join-Path $repoRoot "crates/app-services/tests/linux_e01_integration.rs"
$validationTrustPath = Join-Path $repoRoot "docs/validation-trust-framework.md"
$parserSupportPath = Join-Path $repoRoot "docs/parser-support-matrix.md"
$linuxStage0RegressionPath = Join-Path $repoRoot "docs/real-sample-regression/2026-07-05-linux-stage0-jiancai3.md"

foreach ($path in @(
    $mcpCommandsPath,
    $mcpDtoPath,
    $stagingDir,
    $importPipelineDir,
    $importPipelineModPath,
    $importPipelineEmitPath,
    $analysisExtractionModPath,
    $filePreviewPath,
    $fileCommandsPath,
    $datasourceFacadePath,
    $linuxIntegrationTestPath,
    $validationTrustPath,
    $parserSupportPath,
    $linuxStage0RegressionPath
  )) {
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required Stage 5 regression guard input is missing: $path"
  }
}

$mcpCommands = Get-Content -LiteralPath $mcpCommandsPath -Raw -Encoding UTF8
$mcpDto = Get-Content -LiteralPath $mcpDtoPath -Raw -Encoding UTF8
$staging = (Get-ChildItem -LiteralPath $stagingDir -Filter '*.rs' -File | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 }) -join "`n"
$importPipelineProduction = (Get-ChildItem -LiteralPath $importPipelineDir -Filter '*.rs' -File |
  Where-Object { $_.Name -ne 'tests.rs' } |
  ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 }) -join "`n"
$importPipelineMod = Get-Content -LiteralPath $importPipelineModPath -Raw -Encoding UTF8
$importPipelineEmit = Get-Content -LiteralPath $importPipelineEmitPath -Raw -Encoding UTF8
$analysisExtractionMod = Get-Content -LiteralPath $analysisExtractionModPath -Raw -Encoding UTF8
$filePreview = Get-Content -LiteralPath $filePreviewPath -Raw -Encoding UTF8
$fileCommands = Get-Content -LiteralPath $fileCommandsPath -Raw -Encoding UTF8
$datasourceFacade = Get-Content -LiteralPath $datasourceFacadePath -Raw -Encoding UTF8
$linuxIntegrationTest = Get-Content -LiteralPath $linuxIntegrationTestPath -Raw -Encoding UTF8
$validationTrust = Get-Content -LiteralPath $validationTrustPath -Raw -Encoding UTF8
$parserSupport = Get-Content -LiteralPath $parserSupportPath -Raw -Encoding UTF8
$linuxStage0Regression = Get-Content -LiteralPath $linuxStage0RegressionPath -Raw -Encoding UTF8

function Assert-Matches {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Content -notmatch $Pattern) {
    throw $Message
  }
}

function Assert-NotMatches {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Content -match $Pattern) {
    throw $Message
  }
}

function Assert-NotMatchesCaseSensitive {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Content -cmatch $Pattern) {
    throw $Message
  }
}

# MCP transport validation must reject unknown transports everywhere the command
# layer converts DTOs into concrete MCP transports. In particular, save_mcp_config
# must not silently default an invalid transport to an empty SSE endpoint.
Assert-NotMatches `
  -Content $mcpCommands `
  -Pattern '_\s*=>\s*McpTransport::Sse\s*\{\s*url:\s*String::new\(\)\s*\}' `
  -Message "MCP save/config conversion must not silently default invalid transports to empty SSE"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'fn\s+transport_from_dto[\s\S]{0,800}_\s*=>\s*(?:return\s*)?Err\([\s\S]{0,160}Invalid transport type' `
  -Message "transport_from_dto must reject invalid MCP transport types"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'pub\s+async\s+fn\s+save_mcp_config[\s\S]{0,700}config_from_dto\(config\)\?' `
  -Message "save_mcp_config must use validating MCP config conversion"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'pub\s+async\s+fn\s+add_mcp_server[\s\S]{0,700}server_config_from_dto\(&server\)\?' `
  -Message "add_mcp_server must use validating MCP server conversion"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'pub\s+async\s+fn\s+test_mcp_connection[\s\S]{0,700}Invalid transport type' `
  -Message "test_mcp_connection must validate and report invalid MCP transport types"

foreach ($pattern in @(
    '"sse"\s*=>\s*(?:Ok\()?McpTransport::Sse',
    '"stdio"\s*=>\s*(?:Ok\()?McpTransport::Stdio'
  )) {
  Assert-Matches `
    -Content $mcpCommands `
    -Pattern $pattern `
    -Message "MCP command transport conversion is missing expected arm: $pattern"
}

# Transport DTOs currently serialize as snake_case nested payloads. Guard this
# boundary because the frontend/Tauri command envelope is camelCase, while these
# nested MCP DTOs are not.
foreach ($pattern in @(
    'server_config_serializes_current_snake_case_response_fields',
    'tool_call_request_documents_camel_case_boundary_is_top_level_only',
    'transport_type:\s*String',
    'server_id:\s*String',
    'tool_name:\s*String'
  )) {
  Assert-Matches `
    -Content $mcpDto `
    -Pattern $pattern `
    -Message "MCP transport DTO regression check is missing expected contract marker: $pattern"
}

# Staging merges must not use INSERT OR IGNORE in the merge-to-main path. Silent
# conflict suppression hides missing/duplicate rows and lets staging partitions be
# marked merged even when rows were skipped.
$stagingRepoPath = Join-Path $repoRoot 'crates/persistence-sqlite/src/repositories/staging_repo.rs'
if (-not (Test-Path -LiteralPath $stagingRepoPath)) {
  throw "Required staging repository is missing: $stagingRepoPath"
}
$stagingRepo = Get-Content -LiteralPath $stagingRepoPath -Raw -Encoding UTF8

foreach ($pattern in @(
    'INSERT\s+OR\s+IGNORE\s+INTO\s+main\.file_entries',
    'INSERT\s+OR\s+IGNORE\s+INTO\s+main\.artifacts',
    'INSERT\s+OR\s+IGNORE\s+INTO\s+main\.timeline_events'
  )) {
  Assert-NotMatches `
    -Content $stagingRepo `
    -Pattern $pattern `
    -Message "Staging merge-to-main must not silently suppress conflicts with: $pattern"
}

foreach ($pattern in @(
    'INSERT\s+INTO\s+main\.file_entries',
    'INSERT\s+INTO\s+main\.artifacts',
    'INSERT\s+INTO\s+main\.timeline_events'
  )) {
  Assert-Matches `
    -Content $stagingRepo `
    -Pattern $pattern `
    -Message "Staging merge-to-main is missing explicit insert path: $pattern"
}

# Import pipeline orchestration must remain Tauri-free. Desktop commands may
# adapt ImportEventSink into Tauri events, but app-services must not regain a
# direct dependency on Tauri runtime handles.
Assert-NotMatchesCaseSensitive `
  -Content $importPipelineProduction `
  -Pattern '\btauri::|AppHandle|Window|Emitter|emit_all|emit_to|\.emit\(' `
  -Message "import_pipeline production code must stay Tauri-free; use ImportEventSink adapters at the command boundary"

Assert-Matches `
  -Content $importPipelineEmit `
  -Pattern 'pub\s+trait\s+ImportEventSink' `
  -Message "import_pipeline must expose ImportEventSink as the Tauri-free event boundary"

Assert-Matches `
  -Content $importPipelineEmit `
  -Pattern 'struct\s+NoopImportEventSink' `
  -Message "import_pipeline must keep NoopImportEventSink for tests and non-UI callers"

Assert-Matches `
  -Content $importPipelineMod `
  -Pattern 'pub\s+use\s+emit::\{\s*ImportEventSink,\s*NoopImportEventSink\s*\}' `
  -Message "import_pipeline facade must re-export ImportEventSink and NoopImportEventSink"

# Analysis extraction was split from a god module. Keep the module root as a
# facade and prevent the old runner/preload/summary implementation from
# drifting back into mod.rs.
foreach ($pattern in @(
    'mod\s+runner;',
    'mod\s+registry_preload;',
    'mod\s+summary;',
    'pub\s+use\s+self::runner::run_analysis_extraction',
    'pub\s+use\s+self::summary::\{[\s\S]{0,240}get_linux_artifact_summary'
  )) {
  Assert-Matches `
    -Content $analysisExtractionMod `
    -Pattern $pattern `
    -Message "analysis extraction facade is missing expected split/export marker: $pattern"
}

Assert-NotMatches `
  -Content $analysisExtractionMod `
  -Pattern 'fn\s+(run_analysis_extraction|preload_registry|load_registry|query_artifacts|get_.*summary)\b' `
  -Message "analysis_service/extraction/mod.rs must remain a facade, not regain runner/preload/summary bodies"

# File preview DTO assembly belongs in app-services. The Tauri command layer
# should keep only state/cache/media-protocol adaptation and delegate text,
# image, media, and range reads to file_service.
Assert-Matches `
  -Content $filePreview `
  -Pattern 'Tauri-free preview facade for text, image, and media DTO assembly' `
  -Message "file_service/preview.rs must document and keep the Tauri-free preview assembly boundary"

foreach ($pattern in @(
    'pub\s+fn\s+text_preview_for_file',
    'pub\s+fn\s+image_preview_for_file',
    'pub\s+fn\s+media_preview_plan_for_file',
    'pub\s+fn\s+media_range_for_file',
    'pub\s+fn\s+read_preview_bytes_for_file'
  )) {
  Assert-Matches `
    -Content $filePreview `
    -Pattern $pattern `
    -Message "file_service/preview.rs is missing expected preview service API: $pattern"
}

Assert-NotMatchesCaseSensitive `
  -Content $filePreview `
  -Pattern '\btauri::|AppHandle|Window|Emitter|media_protocol' `
  -Message "file_service/preview.rs must stay Tauri-free; media protocol adaptation belongs in commands"

foreach ($pattern in @(
    'file_service::read_file_range_for_source_case',
    'file_service::image_preview_for_source_case',
    'file_service::media_preview_plan_for_source_case',
    'file_service::media_range_for_source_case',
    'file_service::text_preview_for_source_case'
  )) {
  Assert-Matches `
    -Content $fileCommands `
    -Pattern $pattern `
    -Message "file_commands.rs must delegate preview/range work to app-services: $pattern"
}

# Datasource probing and LVM expansion were split into focused modules. Keep the
# public datasource_service path as a facade so import/viewer callers do not bind
# to private module internals.
foreach ($pattern in @(
    'mod\s+attach;',
    'mod\s+fs_magic;',
    'mod\s+lvm;',
    'mod\s+partition_index;',
    'mod\s+probe;',
    'mod\s+reader;',
    'mod\s+types;',
    'pub\s+use\s+attach::\{[^}]*attach_data_source[^}]*classify_data_source_path',
    'pub\s+use\s+lvm::\{[^}]*expand_lvm_pool_candidates[^}]*expand_lvm_pool_candidates_with_sources',
    'pub\s+use\s+probe::\{[^}]*detect_image_filesystem[^}]*partition_display_name[^}]*volume_display_name',
    'pub\s+use\s+types::\{[\s\S]{0,400}PartitionStatus[\s\S]{0,120}Result'
  )) {
  Assert-Matches `
    -Content $datasourceFacade `
    -Pattern $pattern `
    -Message "datasource_service facade is missing expected split/export marker: $pattern"
}

Assert-NotMatches `
  -Content $datasourceFacade `
  -Pattern 'pub\s+fn\s+(detect_image_filesystem|attach_data_source|classify_data_source_path|expand_lvm_pool_candidates|read_boot_filesystem|open_evidence_reader)\b' `
  -Message "datasource_service.rs must remain a facade; production bodies belong in split modules"

Assert-NotMatches `
  -Content $datasourceFacade `
  -Pattern '\blvm_discovery_sources_for_case\b' `
  -Message "ordinary data-source imports must not regain case-wide LVM supplementary-PV discovery"

# The private Linux single-disk baseline must stay opt-in, documented, and tied
# to FORENSICS_LINUX_E01_FIXTURE. This guards the Linux LVM/XFS acceptance
# surface without committing private evidence.
foreach ($pattern in @(
    'FORENSICS_LINUX_E01_FIXTURE',
    'linux_e01_lvm_expansion_discovers_logical_volumes',
    '#\[ignore\s*=\s*"requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"\]',
    'PartitionStatus::Expanded',
    'cl/root'
  )) {
  Assert-Matches `
    -Content $linuxIntegrationTest `
    -Pattern $pattern `
    -Message "linux_e01_integration.rs is missing expected Linux opt-in baseline marker: $pattern"
}

foreach ($pattern in @(
    'FORENSICS_LINUX_E01_FIXTURE',
    'LVM direct linear/striped LV',
    'XFS root LV',
    'PVE cluster'
  )) {
  Assert-Matches `
    -Content $validationTrust `
    -Pattern $pattern `
    -Message "validation trust framework is missing expected Linux baseline marker: $pattern"
}

foreach ($pattern in @(
    'Linux Stage 0',
    'E01/RAW -> LVM direct LV -> XFS file tree',
    'Beta for private baseline / Experimental for public release',
    'FORENSICS_LINUX_E01_FIXTURE',
    'committed fixture',
    'expected JSON',
    'FORENSICS_PVE_CLUSTER_ROOT',
    'LVM thin/cache/RAID/snapshot/VDO/writecache',
    'partial VG',
    'deleted recovery'
  )) {
  Assert-Matches `
    -Content $parserSupport `
    -Pattern $pattern `
    -Message "parser support matrix is missing expected Linux Stage 0 baseline marker: $pattern"
}

foreach ($pattern in @(
    'Linux Stage 0',
    '164AD86C83AD68137F96D770A5B8A676703ED0B075A6DCEB3ECC61B1FA5D64B4',
    'FORENSICS_LINUX_E01_FIXTURE',
    'linux_e01_lvm_expansion_discovers_logical_volumes',
    'Partition 1 \(LVM\)',
    'Expanded',
    'Partition 2 \(XFS\) - cl/root',
    'files=51261',
    'dirs=7149',
    'CI',
    'baseline',
    'PVE cluster',
    'LVM thin-pool',
    'partial/degraded VG',
    'deleted recovery'
  )) {
  Assert-Matches `
    -Content $linuxStage0Regression `
    -Pattern $pattern `
    -Message "Linux Stage 0 real-sample regression record is missing expected marker: $pattern"
}

Write-Host "Stage 5 regression guard passed: MCP transport validation, nested MCP DTO contract, staging conflict visibility, modular service boundaries, and Linux baseline are locked"
