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

function Count-Lines([string]$Content) {
  if ($Content.Length -eq 0) {
    return 0
  }
  return ($Content -split "`r?`n").Count
}

$facades = @(
  'crates/app-services/src/timeline_service.rs',
  'crates/app-services/src/staging/mod.rs',
  'crates/app-services/src/parallel_enum/mod.rs',
  'crates/app-services/src/import_pipeline/mod.rs',
  'crates/app-services/src/file_service/mod.rs',
  'crates/app-services/src/artifact_service.rs',
  'crates/app-services/src/graph_service.rs',
  'crates/app-services/src/correlation/mod.rs',
  'crates/app-services/src/entity_extraction/mod.rs',
  'crates/app-services/src/entity_resolution/mod.rs',
  'crates/app-services/src/rule_pack/mod.rs'
)

foreach ($relativePath in $facades) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    $errors.Add("missing Stage 4 service facade: $relativePath")
    continue
  }
  $lineCount = Count-Lines (Read-StrictUtf8 $absolutePath)
  if ($lineCount -gt 200) {
    $errors.Add("Stage 4 service facade exceeds 200 lines: $relativePath ($lineCount)")
  }
}

$requiredModules = @(
  'crates/app-services/src/timeline_service/pagination.rs',
  'crates/app-services/src/timeline_service/projection.rs',
  'crates/app-services/src/staging/merge.rs',
  'crates/app-services/src/staging/partition_root.rs',
  'crates/app-services/src/parallel_enum/coordinator.rs',
  'crates/app-services/src/parallel_enum/batch_sink.rs',
  'crates/app-services/src/parallel_enum/ntfs/mft_scan.rs',
  'crates/app-services/src/parallel_enum/ntfs/path_reconstruction.rs',
  'crates/app-services/src/import_pipeline/context.rs',
  'crates/app-services/src/import_pipeline/phases/enumerate.rs',
  'crates/app-services/src/import_pipeline/phases/finalize.rs',
  'crates/app-services/src/import_pipeline/partition/candidates.rs',
  'crates/app-services/src/file_service/metadata/source_routing.rs',
  'crates/app-services/src/file_service/viewer/range/api.rs',
  'crates/app-services/src/file_service/mft/enumeration.rs',
  'crates/app-services/src/artifact_service/persistence.rs',
  'crates/app-services/src/graph_service/source_aggregation.rs',
  'crates/app-services/src/correlation/graph/snapshot.rs',
  'crates/app-services/src/entity_extraction/persistence.rs',
  'crates/app-services/src/entity_resolution/cross_case/matching.rs',
  'crates/app-services/src/rule_pack/engine/execution.rs'
)

foreach ($relativePath in $requiredModules) {
  if (-not (Test-Path -LiteralPath (Join-Path $repoRoot $relativePath) -PathType Leaf)) {
    $errors.Add("missing Stage 4 capability module: $relativePath")
  }
}

$serviceRoot = Join-Path $repoRoot 'crates/app-services/src'
$serviceSources = (
  Get-ChildItem -LiteralPath $serviceRoot -Recurse -File -Filter '*.rs' |
    Sort-Object -Property FullName |
    ForEach-Object { Read-StrictUtf8 $_.FullName }
) -join "`n"

if ($serviceSources -cmatch '\btauri::|\bAppHandle\b|\bWindow\b|\bEmitter\b|\bemit_all\b|\bemit_to\b') {
  $errors.Add('app-services must remain independent from the Tauri runtime')
}

$parallelRoot = Join-Path $repoRoot 'crates/app-services/src/parallel_enum'
$parallelSources = (
  Get-ChildItem -LiteralPath $parallelRoot -Recurse -File -Filter '*.rs' |
    ForEach-Object { Read-StrictUtf8 $_.FullName }
) -join "`n"
if ($parallelSources -match '\bpar_iter\b|\binto_par_iter\b|\brayon::') {
  $errors.Add('parallel_enum must keep evidence-image I/O serial; Rayon belongs in CPU-only transforms')
}

$importSources = (
  Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates/app-services/src/import_pipeline') `
    -Recurse -File -Filter '*.rs' |
    ForEach-Object { Read-StrictUtf8 $_.FullName }
) -join "`n"
if ($importSources -notmatch 'pub\s+trait\s+ImportEventSink' -or
    $importSources -notmatch 'struct\s+NoopImportEventSink') {
  $errors.Add('import_pipeline must retain its Tauri-free ImportEventSink boundary')
}

$sourceRouting = Read-StrictUtf8 (
  Join-Path $repoRoot 'crates/app-services/src/file_service/metadata/source_routing.rs'
)
if ($sourceRouting -notmatch 'GlobalFileId') {
  $errors.Add('file_service source routing must resolve source-scoped file identifiers')
}

$rangeApi = Read-StrictUtf8 (
  Join-Path $repoRoot 'crates/app-services/src/file_service/viewer/range/api.rs'
)
if ($rangeApi -notmatch 'MAX_RANGE_LENGTH') {
  $errors.Add('file viewer range reads must retain the bounded MAX_RANGE_LENGTH clamp')
}

$previewBytes = Read-StrictUtf8 (
  Join-Path $repoRoot 'crates/app-services/src/file_service/viewer/preview_bytes.rs'
)
if ($previewBytes -notmatch 'length\.min\(transport::dto::MAX_VIEWER_RANGE_LENGTH\)') {
  $errors.Add('public preview byte reads must clamp unvalidated lengths to MAX_VIEWER_RANGE_LENGTH')
}

$mediaRange = Read-StrictUtf8 (
  Join-Path $repoRoot 'crates/app-services/src/file_service/viewer/media.rs'
)
if ($mediaRange -notmatch '\.min\(transport::dto::MAX_VIEWER_RANGE_LENGTH\)') {
  $errors.Add('public media range reads must clamp unvalidated lengths to MAX_VIEWER_RANGE_LENGTH')
}

$artifactExtraction = Read-StrictUtf8 (
  Join-Path $repoRoot 'crates/app-services/src/artifact_service/extraction.rs'
)
if ($artifactExtraction -notmatch 'PARALLEL_EXTRACTION_BATCH_SIZE:\s*usize\s*=\s*2' -or
    $artifactExtraction -notmatch 'candidates\.chunks\(PARALLEL_EXTRACTION_BATCH_SIZE\)' -or
    $artifactExtraction -notmatch 'map\(\|file\|\s*prepare_extraction\(file,\s*&file_reader\)\)' -or
    $artifactExtraction -notmatch 'prepared\s*\.into_par_iter\(\)' -or
    $artifactExtraction -notmatch 'collect::<Vec<_>>\(\)') {
  $errors.Add('parallel artifact extraction must use bounded serial evidence reads before CPU-only Rayon work')
}

$artifactPersistence = Read-StrictUtf8 (
  Join-Path $repoRoot 'crates/app-services/src/artifact_service/persistence.rs'
)
if ($artifactPersistence -notmatch 'if\s+let\s+Err\(error\)\s*=\s*populate_artifact_graph') {
  $errors.Add('artifact graph population must remain a non-fatal import side effect')
}

$correlationRoot = Join-Path $repoRoot 'crates/app-services/src/correlation'
$correlationSources = (
  Get-ChildItem -LiteralPath $correlationRoot -Recurse -File -Filter '*.rs' |
    ForEach-Object { Read-StrictUtf8 $_.FullName }
) -join "`n"
if ($correlationSources -notmatch 'source_object_id') {
  $errors.Add('correlation must retain the sourceObjectId bridge')
}

if ($errors.Count -gt 0) {
  Write-Error "Stage 4 service boundary guard failed:`n$($errors -join "`n")"
}

Write-Host 'Stage 4 service boundary guard passed'
