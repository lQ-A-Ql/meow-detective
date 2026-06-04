param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$pipelinePath = Join-Path $repoRoot "apps/desktop/src-tauri/src/commands/import/pipeline.rs"
$stagingPath = Join-Path $repoRoot "crates/app-services/src/staging.rs"
$appStatePath = Join-Path $repoRoot "apps/desktop/src-tauri/src/state/app_state.rs"
$persistenceConnectionPath = Join-Path $repoRoot "crates/persistence-sqlite/src/connection.rs"
$profileScriptPath = Join-Path $repoRoot "scripts/run-e01-import-profile.ps1"
$performanceGatePath = Join-Path $repoRoot "scripts/check-e01-import-performance.ps1"

foreach ($path in @(
    $pipelinePath,
    $stagingPath,
    $appStatePath,
    $persistenceConnectionPath,
    $profileScriptPath,
    $performanceGatePath
  )) {
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required import optimization file is missing: $path"
  }
}

$pipeline = Get-Content -LiteralPath $pipelinePath -Raw -Encoding UTF8
$staging = Get-Content -LiteralPath $stagingPath -Raw -Encoding UTF8
$appState = Get-Content -LiteralPath $appStatePath -Raw -Encoding UTF8
$persistenceConnection = Get-Content -LiteralPath $persistenceConnectionPath -Raw -Encoding UTF8
$profileScript = Get-Content -LiteralPath $profileScriptPath -Raw -Encoding UTF8
$performanceGate = Get-Content -LiteralPath $performanceGatePath -Raw -Encoding UTF8

if ($pipeline -notmatch 'FORENSICS_E01_FIXTURE') {
  throw "desktop real E01 regression must be driven by FORENSICS_E01_FIXTURE"
}
foreach ($forbidden in @('liuyang_pc\.E01', 'E:\\pangushi', '刘洋')) {
  if ($pipeline -match $forbidden) {
    throw "desktop real E01 regression must not hard-code private sample path fragment: $forbidden"
  }
}
if ($pipeline -notmatch 'enable_timeline_projection:\s*!image_backed_source') {
  throw "E01/RAW imports must keep Timeline projection outside the import critical path"
}
if ($pipeline -notmatch 'post-import-skip timeline=deferred content=disabled text=disabled') {
  throw "E01/RAW metadata-only import must emit an explicit deferred post-import profile detail"
}

foreach ($pattern in @(
    'PRAGMA\s+journal_mode\s*=\s*WAL',
    'PRAGMA\s+synchronous\s*=\s*OFF',
    'PRAGMA\s+temp_store\s*=\s*MEMORY',
    'PRAGMA\s+cache_size\s*=\s*-\{STAGING_CACHE_SIZE_KIB\}',
    'PRAGMA\s+mmap_size\s*=\s*\{STAGING_MMAP_SIZE_BYTES\}',
    'STAGING_CACHE_SIZE_KIB:\s*i64\s*=\s*256\s*\*\s*1024',
    'STAGING_MMAP_SIZE_BYTES:\s*i64\s*=\s*256\s*\*\s*1024\s*\*\s*1024'
  )) {
  if ($staging -notmatch $pattern) {
    throw "staging DB aggressive temp pragma is missing or weakened: $pattern"
  }
}
if ($staging -notmatch 'SELECT\s+id,\s+parent_id,\s+data_source_id,\s+path,\s+name,\s+LOWER\(entry_type\)') {
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
    'Timeline events after lazy query',
    'System info: status=Parsed'
  )) {
  if ($performanceGate -notmatch $pattern) {
    throw "real E01 performance gate is missing expected assertion: $pattern"
  }
}

Write-Host "Import optimization guard passed: sample path, timeline deferral, DB pragmas, and profile harness are locked"
