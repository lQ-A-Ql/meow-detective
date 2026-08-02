param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
# The real import profile test lives under the crate's physical tests tree and
# is linked by the test-only bridge in app-services.
$e01PipelineTestPath = Join-Path $repoRoot "crates/app-services/tests/unit/import_pipeline/mod.rs"
# Staging DB pragmas and merge logic moved from app-services/src/staging.rs
# to persistence-sqlite/src/repositories/staging_repo.rs during the
# engineering refactor (commit c24a989 + 1b1a7e3).
$stagingPath = Join-Path $repoRoot "crates/persistence-sqlite/src/repositories/staging_repo.rs"
# Import finalization owns the bounded file-activity Timeline projection.
$pipelineFinalizePath = Join-Path $repoRoot "crates/app-services/src/import_pipeline/phases/finalize.rs"
$timelineProjectionPath = Join-Path $repoRoot "crates/app-services/src/timeline_service/projection.rs"
# Post-import skip logging lives in the analysis worker pool.
$workerPoolPath = Join-Path $repoRoot "crates/app-services/src/import_analysis/worker_pool.rs"
$appStatePath = Join-Path $repoRoot "apps/desktop/src-tauri/src/state/app_state.rs"
$persistenceConnectionPath = Join-Path $repoRoot "crates/persistence-sqlite/src/connection.rs"
$profileScriptPath = Join-Path $repoRoot "scripts/run-e01-import-profile.ps1"
$performanceGatePath = Join-Path $repoRoot "scripts/check-e01-import-performance.ps1"

foreach ($path in @(
    $e01PipelineTestPath,
    $stagingPath,
    $pipelineFinalizePath,
    $timelineProjectionPath,
    $workerPoolPath,
    $appStatePath,
    $persistenceConnectionPath,
    $profileScriptPath,
    $performanceGatePath
  )) {
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required import optimization file is missing: $path"
  }
}

$e01PipelineTest = Get-Content -LiteralPath $e01PipelineTestPath -Raw -Encoding UTF8
$staging = Get-Content -LiteralPath $stagingPath -Raw -Encoding UTF8
$pipelineFinalize = Get-Content -LiteralPath $pipelineFinalizePath -Raw -Encoding UTF8
$timelineProjection = Get-Content -LiteralPath $timelineProjectionPath -Raw -Encoding UTF8
$workerPool = Get-Content -LiteralPath $workerPoolPath -Raw -Encoding UTF8
$appState = Get-Content -LiteralPath $appStatePath -Raw -Encoding UTF8
$persistenceConnection = Get-Content -LiteralPath $persistenceConnectionPath -Raw -Encoding UTF8
$profileScript = Get-Content -LiteralPath $profileScriptPath -Raw -Encoding UTF8
$performanceGate = Get-Content -LiteralPath $performanceGatePath -Raw -Encoding UTF8

if ($e01PipelineTest -notmatch 'FORENSICS_E01_FIXTURE') {
  throw "real E01 import profile must be driven by FORENSICS_E01_FIXTURE"
}
foreach ($forbidden in @('liuyang_pc\.E01', 'E:\\pangushi', '刘洋')) {
  if ($e01PipelineTest -match $forbidden) {
    throw "real E01 import profile must not hard-code private sample path fragment: $forbidden"
  }
}
if ($e01PipelineTest -notmatch 'Timeline events after import') {
  throw "real E01 import profile must verify the import-finalized Timeline projection"
}
if ($pipelineFinalize -notmatch 'ImportContentKind::Filesystem' -or
    $pipelineFinalize -notmatch 'timeline_service::materialize_file_activity\(') {
  throw "Filesystem imports must finalize the narrowed file-activity Timeline projection before becoming ready"
}
foreach ($pattern in @(
    'const\s+SOURCE_BATCH_SIZE:\s*u32\s*=\s*10_000',
    'insert_file_activity_batched',
    'DataSourcePlatform::Windows',
    'DataSourcePlatform::Linux'
  )) {
  if ($timelineProjection -notmatch $pattern) {
    throw "Timeline finalization lost its bounded or platform-specific projection policy: $pattern"
  }
}
if ($workerPool -notmatch 'timeline=finalize content=disabled text=disabled') {
  throw "E01/RAW metadata-only import must report Timeline finalization explicitly"
}

foreach ($pattern in @(
    'PRAGMA\s+journal_mode\s*=\s*WAL',
    'PRAGMA\s+synchronous\s*=\s*OFF',
    'PRAGMA\s+temp_store\s*=\s*MEMORY',
    'PRAGMA\s+cache_size\s*=\s*-\{STAGING_CACHE_SIZE_KIB\}',
    'PRAGMA\s+mmap_size\s*=\s*\{STAGING_MMAP_SIZE_BYTES\}',
    'STAGING_CACHE_SIZE_KIB:\s*i64\s*=\s*16\s*\*\s*1024',
    'STAGING_MMAP_SIZE_BYTES:\s*i64\s*=\s*64\s*\*\s*1024\s*\*\s*1024'
  )) {
  if ($staging -notmatch $pattern) {
    throw "staging DB aggressive temp pragma is missing or weakened: $pattern"
  }
}
if ($staging -notmatch 'SELECT[\s\S]*?LOWER\(entry_type\)[\s\S]*?FROM\s+staging\.file_entries') {
  throw "enum staging merge must normalize entry_type to lowercase before writing app.db"
}

foreach ($mainDbSource in @($appState, $persistenceConnection)) {
  if ($mainDbSource -match 'PRAGMA\s+synchronous\s*=\s*OFF') {
    throw "main app.db path must not use synchronous=OFF"
  }
}
if ($appState -notmatch 'PRAGMA\s+synchronous\s*=\s*NORMAL') {
  throw "desktop app.db connection should keep synchronous=NORMAL"
}
if ($persistenceConnection -notmatch 'PRAGMA\s+synchronous\s*=\s*NORMAL') {
  throw "persistence app.db connection should keep synchronous=NORMAL"
}

foreach ($pattern in @(
    '\[int\]\$Runs\s*=\s*3',
    'FORENSICS_E01_FIXTURE',
    '\\?\[import-profile\\?\]',
    'totalMedianSeconds',
    'enumerationMedianSeconds',
    'rssMaxMb',
    'artifacts/import-profiles'
  )) {
  if ($profileScript -notmatch $pattern) {
    throw "real E01 profile harness is missing expected behavior: $pattern"
  }
}

foreach ($pattern in @(
    'MaxTotalMedianSeconds',
    'MaxEnumerationMedianSeconds',
    'MaxRssMb',
    'MinRowsPerSec',
    'run-e01-import-profile\.ps1',
    'NTFS shape: root Windows=\\d\+, root System32=0',
    'Timeline events after import',
    'System info: status=Parsed'
  )) {
  if ($performanceGate -notmatch $pattern) {
    throw "real E01 performance gate is missing expected assertion: $pattern"
  }
}

Write-Host "Import optimization guard passed: sample path, bounded Timeline finalization, DB pragmas, and profile harness are locked"
