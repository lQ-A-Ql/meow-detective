param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot 'lib/RustGuard.Common.ps1')
# The real import profile test lives under the crate's physical tests tree and
# is linked by the test-only bridge in app-services.
$e01PipelineTestPath = Join-Path $repoRoot "crates/app-services/tests/unit/import_pipeline/mod.rs"
# Staging DB pragmas and merge logic moved from app-services/src/staging.rs
# to persistence-sqlite/src/repositories/staging_repo.rs during the
# engineering refactor (commit c24a989 + 1b1a7e3), then to
# persistence-source/src/repositories/staging_repo.rs in S4b.
$stagingPath = Join-Path $repoRoot "crates/persistence-source/src/repositories/staging_repo.rs"
$catalogMergePath = Join-Path $repoRoot "crates/persistence-source/src/staging/catalog.rs"
$catalogSqlPath = Join-Path $repoRoot "crates/persistence-source/src/staging/catalog_staging.sql"
# Import finalization owns the bounded file-activity Timeline projection.
$pipelineFinalizePath = Join-Path $repoRoot "crates/import-core/src/import_pipeline/phases/finalize.rs"
$timelineProjectionPath = Join-Path $repoRoot "crates/import-core/src/timeline_projection/fjall.rs"
# The streamed bounded prefix-replacement primitive moved from import-core's
# timeline projection into the persistence-fjall TimelineRecordStore during
# the 63f9e66e constant-cost projection refactor; import-core now pages
# through the segmented variant on top of it.
$timelineBatchPath = Join-Path $repoRoot "crates/persistence-fjall/src/timeline/timeline_batch.rs"
# Post-import skip logging lives in the analysis worker pool.
$workerPoolPath = Join-Path $repoRoot "crates/import-core/src/import_analysis/worker_pool.rs"
$appStatePath = Join-Path $repoRoot "apps/desktop/src-tauri/src/state/app_state.rs"
$caseDatabasePath = Join-Path $repoRoot "apps/desktop/src-tauri/src/state/case_database.rs"
$persistenceConnectionPath = Join-Path $repoRoot "crates/persistence-sqlite/src/connection.rs"
$profileScriptPath = Join-Path $repoRoot "scripts/run-e01-import-profile.ps1"
$performanceGatePath = Join-Path $repoRoot "scripts/check-e01-import-performance.ps1"

foreach ($path in @(
    $e01PipelineTestPath,
    $stagingPath,
    $catalogMergePath,
    $catalogSqlPath,
    $pipelineFinalizePath,
    $timelineProjectionPath,
    $timelineBatchPath,
    $workerPoolPath,
    $appStatePath,
    $caseDatabasePath,
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
$catalogMerge = Get-Content -LiteralPath $catalogMergePath -Raw -Encoding UTF8
$catalogSql = Get-Content -LiteralPath $catalogSqlPath -Raw -Encoding UTF8
$pipelineFinalize = Get-Content -LiteralPath $pipelineFinalizePath -Raw -Encoding UTF8
$timelineProjection = Get-Content -LiteralPath $timelineProjectionPath -Raw -Encoding UTF8
$timelineBatch = Get-Content -LiteralPath $timelineBatchPath -Raw -Encoding UTF8
$workerPool = Get-Content -LiteralPath $workerPoolPath -Raw -Encoding UTF8
$appState = Get-Content -LiteralPath $appStatePath -Raw -Encoding UTF8
$caseDatabase = Get-Content -LiteralPath $caseDatabasePath -Raw -Encoding UTF8
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
    $pipelineFinalize -notmatch 'timeline_projection_mark_deferred' -or
    $pipelineFinalize -notmatch 'queue_timeline\(') {
  throw "Filesystem imports must queue timeline work after catalog publication"
}
foreach ($pattern in @(
    'catalog\.list_all\(',
    'ScanDirection::Forward',
    '512',
    'DataSourcePlatform::Windows',
    'platform\s*!=\s*DataSourcePlatform::Windows'
  )) {
  if ($timelineProjection -notmatch $pattern) {
    throw "Timeline finalization lost its bounded or platform-specific projection policy: $pattern"
  }
}
if ($timelineBatch -notmatch 'replace_by_parser_prefix_streamed') {
  throw "Timeline finalization lost its bounded streamed prefix replacement primitive in persistence-fjall: replace_by_parser_prefix_streamed"
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
if ($staging -notmatch 'crate::staging::merge_staging_catalog\(' -or
    $catalogMerge -notmatch 'repository\.upsert_parallel\(' -or
    $catalogSql -notmatch 'FROM\s+file_entries') {
  throw "enumeration staging must publish records through the native Catalog repository"
}
if ($staging -match 'ATTACH DATABASE|INSERT INTO main\.file_entries|staging_enum_merge|merge_completed_numeric|ALTER TABLE') {
  throw "enumeration staging contains a retired SQLite detail merge or schema upgrade"
}

foreach ($mainDbSource in @($appState, $caseDatabase, $persistenceConnection)) {
  if ($mainDbSource -match 'PRAGMA\s+synchronous\s*=\s*OFF') {
    throw "main app.db path must not use synchronous=OFF"
  }
}
$caseDatabaseCode = [Stage0.RustGuardLexicalMasker]::Mask($caseDatabase)
$transitionConnection = [regex]::Match(
  $caseDatabaseCode,
  '(?ms)^    pub\(crate\) fn get_transition_connection\b.*?^    \}'
)
if (-not $transitionConnection.Success -or
    $transitionConnection.Value -notmatch 'persistence_sqlite::connection::open_existing\s*\(\s*&db_path\s*\)') {
  throw "desktop app.db connections must use the canonical persistence open_existing policy"
}
foreach ($method in @('init_db_pragmas', 'get_connection')) {
  $methodBody = [regex]::Match(
    $caseDatabaseCode,
    "(?ms)^    pub fn $method\b.*?^    \}"
  )
  if (-not $methodBody.Success -or
      $methodBody.Value -notmatch 'self\.get_transition_connection\s*\(') {
    throw "desktop $method must delegate to the canonical writer connection"
  }
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
