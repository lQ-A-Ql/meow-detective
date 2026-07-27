param()

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$strictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)
$errors = New-Object System.Collections.Generic.List[string]

function Read-StrictUtf8([string]$Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
    throw "UTF-8 BOM is not allowed: $Path"
  }
  return $strictUtf8.GetString($bytes)
}

$requiredPaths = @(
  'crates/app-services/src/analysis_service/capability.rs',
  'crates/app-services/src/analysis_service/platforms/mod.rs',
  'crates/app-services/src/analysis_service/platforms/windows.rs',
  'crates/app-services/src/analysis_service/platforms/linux.rs',
  'crates/app-services/src/analysis_service/platforms/source.rs',
  'crates/app-services/src/analysis_service/use_cases/mod.rs',
  'crates/app-services/src/analysis_service/use_cases/source.rs',
  'crates/app-services/src/analysis_service/candidates.rs',
  'crates/app-services/src/analysis_service/extraction/linux.rs',
  'crates/app-services/src/import_analysis/extractor_policy.rs',
  'crates/app-services/src/report/source_analysis.rs',
  'crates/app-services/src/source_db/ready.rs',
  'crates/app-services/src/v3_governance_service/artifact_family_platform.rs'
)

foreach ($relativePath in $requiredPaths) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    $errors.Add("missing Stage 2 platform module: $relativePath")
  }
}

$serviceRoot = Join-Path $repoRoot 'crates/app-services/src'
foreach ($file in Get-ChildItem -LiteralPath $serviceRoot -Recurse -File -Filter '*.rs') {
  $content = Read-StrictUtf8 $file.FullName
  $relative = $file.FullName.Substring($repoRoot.Length + 1).Replace('\', '/')
  if ($content -match '\bImportTargetPlatformDto\b') {
    $errors.Add("app-services depends on transport platform DTO: $relative")
  }
  if (-not $relative.StartsWith('crates/app-services/src/source_db') -and
      $content.Contains('open_registered_source_db(')) {
    $errors.Add("investigator-facing service bypasses the typed ready-source route: $relative")
  }
}

$analysisCommandPath = Join-Path $repoRoot 'apps/desktop/src-tauri/src/commands/analysis_commands.rs'
if (Test-Path -LiteralPath $analysisCommandPath -PathType Leaf) {
  $analysisCommandParts = New-Object System.Collections.Generic.List[string]
  $analysisCommandParts.Add((Read-StrictUtf8 $analysisCommandPath))
  $analysisCommandDirectory = Join-Path $repoRoot 'apps/desktop/src-tauri/src/commands/analysis_commands'
  if (Test-Path -LiteralPath $analysisCommandDirectory -PathType Container) {
    foreach ($analysisFile in Get-ChildItem -LiteralPath $analysisCommandDirectory -Recurse -File -Filter '*.rs') {
      $analysisCommandParts.Add((Read-StrictUtf8 $analysisFile.FullName))
    }
  }
  $analysisCommand = $analysisCommandParts -join "`n"
  foreach ($forbidden in @(
    'DataSourceRepo',
    'analysis_source_platform',
    'ensure_categories_match_source_platform',
    'is_linux_analysis_category',
    'resolve_data_source_platform',
    'select_evidence_scan_categories',
    'open_registered_source_db',
    'run_targeted_evidence_scan',
    'extract_system_info_for_case',
    'classify_files_by_metadata',
    'analysis_service::run_analysis_extraction'
  )) {
    if ($analysisCommand.Contains($forbidden)) {
      $errors.Add("analysis command retains platform business logic: $forbidden")
    }
  }
  foreach ($required in @(
    'run_active_case_command(',
    'analysis_service::get_source_system_info(',
    'analysis_service::run_source_evidence_scan(',
    'analysis_service::generate_source_analysis_summary('
  )) {
    if (-not $analysisCommand.Contains($required)) {
      $errors.Add("analysis command is missing a thin service delegation: $required")
    }
  }
  $extractionDelegations = @(
    'analysis_service::run_source_analysis_extraction(',
    'analysis_service::run_source_analysis_extraction_with_progress('
  )
  if (-not ($extractionDelegations | Where-Object { $analysisCommand.Contains($_) })) {
    $errors.Add('analysis command is missing a thin service delegation: source analysis extraction')
  }
}

$analysisSourceUseCasePath = Join-Path $repoRoot 'crates/app-services/src/analysis_service/use_cases/source.rs'
if (Test-Path -LiteralPath $analysisSourceUseCasePath -PathType Leaf) {
  $analysisSourceUseCase = Read-StrictUtf8 $analysisSourceUseCasePath
  foreach ($required in @(
    'open_ready_source_by_id(case_conn, case_root, case_id, data_source_id)?'
  )) {
    if (-not $analysisSourceUseCase.Contains($required)) {
      $errors.Add("analysis source use case bypasses ready/platform routing: $required")
    }
  }
}

$evidencePolicyPath = Join-Path $repoRoot 'crates/app-services/src/analysis_service/platforms/evidence.rs'
if (Test-Path -LiteralPath $evidencePolicyPath -PathType Leaf) {
  $evidencePolicy = Read-StrictUtf8 $evidencePolicyPath
  foreach ($required in @(
    'evidence_summary_category_allowed',
    'platform != DataSourcePlatform::Windows',
    'use run_analysis_extraction for Linux'
  )) {
    if (-not $evidencePolicy.Contains($required)) {
      $errors.Add("analysis evidence policy is missing platform isolation: $required")
    }
  }
}

$candidateSummaryPath = Join-Path $repoRoot 'crates/app-services/src/analysis_service/candidates/summary.rs'
if (Test-Path -LiteralPath $candidateSummaryPath -PathType Leaf) {
  $candidateSummary = Read-StrictUtf8 $candidateSummaryPath
  foreach ($required in @(
    'platform: DataSourcePlatform',
    'evidence_summary_category_allowed(platform, definition.category)?'
  )) {
    if (-not $candidateSummary.Contains($required)) {
      $errors.Add("evidence summary is not scoped to the persisted platform: $required")
    }
  }
}

$candidateCommonPath = Join-Path $repoRoot 'crates/app-services/src/analysis_service/candidates/common.rs'
if (Test-Path -LiteralPath $candidateCommonPath -PathType Leaf) {
  $candidateCommon = Read-StrictUtf8 $candidateCommonPath
  if ($candidateCommon -match 'super::windows|windows::') {
    $errors.Add('platform-neutral candidate discovery depends directly on Windows rules')
  }
}

$importPhasesPath = Join-Path $repoRoot 'crates/app-services/src/import_pipeline/phases.rs'
if (Test-Path -LiteralPath $importPhasesPath -PathType Leaf) {
  $importPhases = Read-StrictUtf8 $importPhasesPath
  foreach ($forbidden in @(
    'lvm_discovery_sources_for_case',
    'expand_lvm_pool_candidates_with_sources'
  )) {
    if ($importPhases.Contains($forbidden)) {
      $errors.Add("ordinary import pipeline can aggregate LVM PVs across sources: $forbidden")
    }
  }
  if (-not $importPhases.Contains('platform: ctx.import_config.platform')) {
    $errors.Add('post-import analysis does not receive the persisted data-source platform')
  }
}

$extractorPolicyPath = Join-Path $repoRoot 'crates/app-services/src/import_analysis/extractor_policy.rs'
if (Test-Path -LiteralPath $extractorPolicyPath -PathType Leaf) {
  $extractorPolicy = Read-StrictUtf8 $extractorPolicyPath
  foreach ($required in @(
    'DataSourcePlatform::Windows => Ok(Self',
    'DataSourcePlatform::Linux => Ok(Self { registry: None })',
    'DataSourcePlatform::Unknown => Err(unsupported_platform(platform))'
  )) {
    if (-not $extractorPolicy.Contains($required)) {
      $errors.Add("post-import extractor policy is not fail-closed by platform: $required")
    }
  }
}

foreach ($relativePath in @(
  'crates/app-services/src/import_analysis/task_feed.rs',
  'crates/app-services/src/import_analysis/worker_runtime.rs'
)) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    continue
  }
  $content = Read-StrictUtf8 $absolutePath
  if (-not $content.Contains('PlatformExtractorPolicy::for_platform(options.platform)?')) {
    $errors.Add("post-import producer/worker bypasses the shared platform policy: $relativePath")
  }
  if ($content.Contains('artifact_service::create_registry()')) {
    $errors.Add("post-import producer/worker constructs a Windows registry directly: $relativePath")
  }
}

$reportSourceAnalysisPath = Join-Path $repoRoot 'crates/app-services/src/report/source_analysis.rs'
if (Test-Path -LiteralPath $reportSourceAnalysisPath -PathType Leaf) {
  $reportSourceAnalysis = Read-StrictUtf8 $reportSourceAnalysisPath
  foreach ($required in @(
    'source_db::ready_data_sources',
    'source_db::open_ready_source_by_id',
    'source.platform == DataSourcePlatform::Windows',
    'unavailable_windows_system_info'
  )) {
    if (-not $reportSourceAnalysis.Contains($required)) {
      $errors.Add("case report analysis is missing source/platform isolation: $required")
    }
  }
  if ($reportSourceAnalysis.Contains('DataSourceRepo')) {
    $errors.Add('case report analysis duplicates the shared ready-source repository policy')
  }
}

$realSampleTestPath = Join-Path $repoRoot 'apps/desktop/src-tauri/tests/dual_source_import.rs'
$realSampleGatePath = Join-Path $repoRoot 'scripts/check-stage2-real-sample-isolation.ps1'
foreach ($requiredPath in @($realSampleTestPath, $realSampleGatePath)) {
  if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
    $relative = $requiredPath.Substring($repoRoot.Length + 1).Replace('\', '/')
    $errors.Add("missing Stage 2 real-sample isolation surface: $relative")
  }
}
if ((Test-Path -LiteralPath $realSampleTestPath -PathType Leaf)) {
  $realSampleTest = Read-StrictUtf8 $realSampleTestPath
  foreach ($required in @(
    'FORENSICS_STAGE2_WINDOWS_E01',
    'FORENSICS_STAGE2_LINUX_E01',
    'real_samples_import_into_isolated_source_databases_serially',
    'real_samples_remain_isolated_when_linux_imports_first'
  )) {
    if (-not $realSampleTest.Contains($required)) {
      $errors.Add("Stage 2 real-sample regression is missing marker: $required")
    }
  }
  if ($realSampleTest -match '[A-Za-z]:\\') {
    $errors.Add('Stage 2 real-sample regression hard-codes a private fixture path')
  }
}
if ((Test-Path -LiteralPath $realSampleGatePath -PathType Leaf)) {
  $realSampleGate = Read-StrictUtf8 $realSampleGatePath
  foreach ($required in @('-RequireFixtures', "'both'", '--test-threads=1')) {
    if (-not $realSampleGate.Contains($required)) {
      $errors.Add("Stage 2 real-sample gate is missing explicit behavior: $required")
    }
  }
}

$familyPlatformPath = Join-Path $repoRoot 'crates/app-services/src/v3_governance_service/artifact_family_platform.rs'
if (Test-Path -LiteralPath $familyPlatformPath -PathType Leaf) {
  $familyPlatform = Read-StrictUtf8 $familyPlatformPath
  foreach ($required in @(
    '"LinuxJournal"',
    '"LinuxMysqlFinding"',
    'ArtifactFamilyPlatform::Unknown'
  )) {
    if (-not $familyPlatform.Contains($required)) {
      $errors.Add("V3 governance platform classification is incomplete: $required")
    }
  }
}

$sourceDbPath = Join-Path $repoRoot 'crates/app-services/src/source_db.rs'
if (Test-Path -LiteralPath $sourceDbPath -PathType Leaf) {
  $sourceDb = Read-StrictUtf8 $sourceDbPath
  foreach ($required in @(
    'open_ready_source_by_id, open_ready_source_connections,',
    'open_ready_source_connections_read_only, open_ready_source_read_only_by_id'
  )) {
    if (-not $sourceDb.Contains($required)) {
      $errors.Add("source database facade does not expose the ready-state-isolated route: $required")
    }
  }
}

$readySourcePath = Join-Path $repoRoot 'crates/app-services/src/source_db/ready.rs'
if (Test-Path -LiteralPath $readySourcePath -PathType Leaf) {
  $readySource = Read-StrictUtf8 $readySourcePath
  foreach ($required in @(
    'pub fn open_ready_source_by_id(',
    'pub fn open_ready_source_connections(',
    'pub fn open_ready_source_connections_read_only(',
    'open_ready_source_connections_with(case_conn, case_root, case_id, open_ready_source_by_id)',
    'let sources = super::ready_data_sources(case_conn, case_id)?;',
    '.find_by_case(case_id)?',
    'eq_ignore_ascii_case("ready")',
    'DataSourcePlatform::parse_explicit(&storage.platform)'
  )) {
    if (-not $readySource.Contains($required)) {
      $errors.Add("ready-source route is missing a fail-closed check: $required")
    }
  }
} else {
  $errors.Add('missing typed ready-source route: crates/app-services/src/source_db/ready.rs')
}

foreach ($relativePath in @(
  'crates/app-services/src/artifact_service/aggregation.rs',
  'crates/app-services/src/timeline_service/pagination.rs',
  'crates/app-services/src/graph_service/source_aggregation.rs',
  'crates/app-services/src/correlation/graph/snapshot.rs',
  'crates/app-services/src/case_service.rs',
  'crates/app-services/src/file_service/data_sources.rs',
  'crates/app-services/src/file_service/metadata/source_routing.rs',
  'crates/app-services/src/search_service/case_search.rs',
  'crates/app-services/src/step_recorder.rs'
)) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (Test-Path -LiteralPath $absolutePath -PathType Leaf) {
    $content = Read-StrictUtf8 $absolutePath
    if (-not $content.Contains('open_ready_source_connections(') -and
        -not $content.Contains('open_ready_source_connections_read_only(') -and
        -not $content.Contains('ready_data_sources(')) {
      $errors.Add("case-wide aggregation bypasses the shared ready-source router: $relativePath")
    }
    if ($content -match 'import_state\s*==\s*"failed"') {
      $errors.Add("case-wide aggregation still treats non-failed partial sources as readable: $relativePath")
    }
  }
}

foreach ($relativePath in @(
  'crates/app-services/src/report/mod.rs',
  'crates/app-services/src/report/csv.rs',
  'crates/app-services/src/report/json_case.rs'
)) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if ((Test-Path -LiteralPath $absolutePath -PathType Leaf) -and
      -not (Read-StrictUtf8 $absolutePath).Contains('current_governance_for_case')) {
    $errors.Add("case-aware report uses case-DB-only governance: $relativePath")
  }
}

$frontendFilesTypePath = Join-Path $repoRoot 'frontend/src/types/files.ts'
if (Test-Path -LiteralPath $frontendFilesTypePath -PathType Leaf) {
  $frontendFilesType = Read-StrictUtf8 $frontendFilesTypePath
  if ($frontendFilesType -notmatch "ImportTargetPlatform\s*=\s*'windows'\s*\|\s*'linux'") {
    $errors.Add('frontend import platform type is not restricted to windows | linux')
  }
  if ($frontendFilesType -match 'platform\?\s*:\s*ImportTargetPlatform') {
    $errors.Add('frontend import request platform is optional')
  }
}

$analysisStorePath = Join-Path $repoRoot 'frontend/src/stores/analysis-store.ts'
$analysisWorkspacePath = Join-Path $repoRoot 'frontend/src/features/analysis/components/AnalysisWorkspace.tsx'
foreach ($path in @($analysisStorePath, $analysisWorkspacePath)) {
  if ((Test-Path -LiteralPath $path -PathType Leaf) -and (Read-StrictUtf8 $path).Contains('activePlatformView')) {
    $relative = $path.Substring($repoRoot.Length + 1).Replace('\', '/')
    $errors.Add("frontend retains redundant platform-view state: $relative")
  }
}

if (Test-Path -LiteralPath $analysisWorkspacePath -PathType Leaf) {
  $analysisWorkspaceLines = (Read-StrictUtf8 $analysisWorkspacePath).Split("`n").Count
  if ($analysisWorkspaceLines -gt 500) {
    $errors.Add("analysis workspace exceeds the frontend component limit: $analysisWorkspaceLines lines")
  }
}

foreach ($relativePath in @(
  'frontend/src/features/analysis/components/WindowsAnalysisView.tsx',
  'frontend/src/features/analysis/components/LinuxAnalysisView.tsx',
  'frontend/src/features/analysis/types.ts'
)) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    $errors.Add("missing platform-scoped frontend analysis module: $relativePath")
    continue
  }
  if ($relativePath.EndsWith('View.tsx') -and (Read-StrictUtf8 $absolutePath).Contains('@/stores/')) {
    $errors.Add("platform view imports global store directly: $relativePath")
  }
}

foreach ($relativePath in @(
  'crates/app-services/src/analysis_service/candidates.rs',
  'crates/app-services/src/analysis_service/extraction/linux.rs'
)) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    continue
  }
  $lineCount = (Read-StrictUtf8 $absolutePath).Split("`n").Count
  if ($lineCount -gt 200) {
    $errors.Add("Stage 2 facade exceeds 200 lines: $relativePath ($lineCount)")
  }
}

if ($errors.Count -gt 0) {
  throw ("Stage 2 platform boundary guard failed:`n- " + ($errors -join "`n- "))
}

Write-Host 'Stage 2 platform boundary guard passed: domain platform, thin command, symmetric analyzers, and facades are locked'
