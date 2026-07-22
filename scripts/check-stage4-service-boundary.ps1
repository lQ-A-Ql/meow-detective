param(
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib/RustGuard.Common.ps1')

$strictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)
$errors = New-Object System.Collections.Generic.List[string]

function Read-StrictUtf8([string]$Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -ge 3 -and
      $bytes[0] -eq 0xEF -and
      $bytes[1] -eq 0xBB -and
      $bytes[2] -eq 0xBF) {
    throw "UTF-8 BOM is not allowed: $Path"
  }
  return $strictUtf8.GetString($bytes)
}

function Get-LineCount([string]$Content) {
  if ($Content.Length -eq 0) {
    return 0
  }
  $count = [regex]::Matches($Content, "\r\n|\n|\r").Count
  if (-not $Content.EndsWith("`n") -and -not $Content.EndsWith("`r")) {
    $count++
  }
  return $count
}

function Get-MaskedRust([string]$Content) {
  return [Stage0.RustGuardLexicalMasker]::Mask($Content)
}

function Test-ModuleDeclaration(
  [string]$MaskedContent,
  [string]$ModuleName
) {
  $escaped = [regex]::Escape($ModuleName)
  return [regex]::IsMatch(
    $MaskedContent,
    "(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+$escaped\s*;"
  )
}

function Test-ForbiddenTauriDependency([string]$Name) {
  return $Name -match '^tauri(?:$|-)|^tauri-runtime(?:$|-)|^tauri-plugin-'
}

function Assert-MaskedPattern(
  [string]$RelativePath,
  [string]$Pattern,
  [string]$Message
) {
  $absolutePath = Join-Path $repoRoot $RelativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    $errors.Add("missing Stage 4 invariant source: $RelativePath")
    return
  }
  $masked = Get-MaskedRust (Read-StrictUtf8 $absolutePath)
  if (-not [regex]::IsMatch(
      $masked,
      $Pattern,
      [System.Text.RegularExpressions.RegexOptions]::Singleline
    )) {
    $errors.Add($Message)
  }
}

function Invoke-SelfTest {
  if ((Get-LineCount "alpha`n") -ne 1 -or
      (Get-LineCount "alpha`nbeta") -ne 2 -or
      (Get-LineCount '') -ne 0) {
    throw 'Stage 4 self-test failed: line counting includes a trailing empty line'
  }

  $maskedFake = Get-MaskedRust @'
// GlobalFileId::parse(value)
let marker = "GlobalFileId::parse(value)";
'@
  if ($maskedFake -match 'GlobalFileId') {
    throw 'Stage 4 self-test failed: comments or strings can satisfy token checks'
  }

  $maskedReal = Get-MaskedRust 'let id = GlobalFileId::parse(value);'
  if ($maskedReal -notmatch 'GlobalFileId') {
    throw 'Stage 4 self-test failed: executable Rust tokens were masked'
  }

  if (-not (Test-ModuleDeclaration 'pub(crate) mod raw_bundle;' 'raw_bundle') -or
      (Test-ModuleDeclaration '// mod raw_bundle;' 'raw_bundle')) {
    throw 'Stage 4 self-test failed: module wiring detection is incorrect'
  }

  if (-not (Test-ForbiddenTauriDependency 'tauri-plugin-dialog') -or
      -not (Test-ForbiddenTauriDependency 'tauri-runtime') -or
      (Test-ForbiddenTauriDependency 'transport')) {
    throw 'Stage 4 self-test failed: Tauri dependency classification is incorrect'
  }

  Write-Host 'Stage 4 service boundary self-test passed'
}

if ($SelfTest) {
  Invoke-SelfTest
  exit 0
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
  'crates/app-services/src/rule_pack/mod.rs',
  'crates/app-services/src/analysis_service/extraction/browser.rs',
  'crates/app-services/src/analysis_service/extraction/registry/mod.rs',
  'crates/app-services/src/analysis_service/extraction/summary/linux.rs',
  'crates/app-services/src/analysis_service/extraction/summary/registry.rs',
  'crates/app-services/src/datasource_service/lvm.rs',
  'crates/app-services/src/governance/scoring.rs',
  'crates/app-services/src/import_analysis/mod.rs',
  'crates/app-services/src/notebook_service/mod.rs',
  'crates/app-services/src/report/json.rs',
  'crates/app-services/src/report/mod.rs'
)

foreach ($relativePath in $facades) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    $errors.Add("missing Stage 4 service facade: $relativePath")
    continue
  }
  $lineCount = Get-LineCount (Read-StrictUtf8 $absolutePath)
  if ($lineCount -gt 200) {
    $errors.Add("Stage 4 service facade exceeds 200 lines: $relativePath ($lineCount)")
  }
}

$moduleManifest = @(
  @('crates/app-services/src/timeline_service.rs', 'pagination', 'crates/app-services/src/timeline_service/pagination.rs'),
  @('crates/app-services/src/timeline_service.rs', 'projection', 'crates/app-services/src/timeline_service/projection.rs'),
  @('crates/app-services/src/staging/mod.rs', 'merge', 'crates/app-services/src/staging/merge.rs'),
  @('crates/app-services/src/staging/mod.rs', 'partition_root', 'crates/app-services/src/staging/partition_root.rs'),
  @('crates/app-services/src/parallel_enum/mod.rs', 'coordinator', 'crates/app-services/src/parallel_enum/coordinator.rs'),
  @('crates/app-services/src/parallel_enum/mod.rs', 'batch_sink', 'crates/app-services/src/parallel_enum/batch_sink.rs'),
  @('crates/app-services/src/parallel_enum/ntfs/mod.rs', 'mft_scan', 'crates/app-services/src/parallel_enum/ntfs/mft_scan.rs'),
  @('crates/app-services/src/parallel_enum/ntfs/mod.rs', 'path_reconstruction', 'crates/app-services/src/parallel_enum/ntfs/path_reconstruction.rs'),
  @('crates/app-services/src/import_pipeline/mod.rs', 'context', 'crates/app-services/src/import_pipeline/context.rs'),
  @('crates/app-services/src/import_pipeline/phases/mod.rs', 'enumerate', 'crates/app-services/src/import_pipeline/phases/enumerate.rs'),
  @('crates/app-services/src/import_pipeline/phases/mod.rs', 'finalize', 'crates/app-services/src/import_pipeline/phases/finalize.rs'),
  @('crates/app-services/src/import_pipeline/partition/mod.rs', 'candidates', 'crates/app-services/src/import_pipeline/partition/candidates.rs'),
  @('crates/app-services/src/file_service/metadata/mod.rs', 'source_routing', 'crates/app-services/src/file_service/metadata/source_routing.rs'),
  @('crates/app-services/src/file_service/viewer/range/mod.rs', 'api', 'crates/app-services/src/file_service/viewer/range/api.rs'),
  @('crates/app-services/src/file_service/mft/mod.rs', 'enumeration', 'crates/app-services/src/file_service/mft/enumeration.rs'),
  @('crates/app-services/src/artifact_service.rs', 'persistence', 'crates/app-services/src/artifact_service/persistence.rs'),
  @('crates/app-services/src/graph_service.rs', 'source_aggregation', 'crates/app-services/src/graph_service/source_aggregation.rs'),
  @('crates/app-services/src/correlation/graph.rs', 'snapshot', 'crates/app-services/src/correlation/graph/snapshot.rs'),
  @('crates/app-services/src/entity_extraction/mod.rs', 'persistence', 'crates/app-services/src/entity_extraction/persistence.rs'),
  @('crates/app-services/src/entity_resolution/cross_case/mod.rs', 'matching', 'crates/app-services/src/entity_resolution/cross_case/matching.rs'),
  @('crates/app-services/src/rule_pack/engine/mod.rs', 'execution', 'crates/app-services/src/rule_pack/engine/execution.rs'),
  @('crates/app-services/src/analysis_service/extraction/browser.rs', 'chromium', 'crates/app-services/src/analysis_service/extraction/browser/chromium.rs'),
  @('crates/app-services/src/analysis_service/extraction/browser.rs', 'firefox', 'crates/app-services/src/analysis_service/extraction/browser/firefox.rs'),
  @('crates/app-services/src/analysis_service/extraction/browser.rs', 'profile', 'crates/app-services/src/analysis_service/extraction/browser/profile.rs'),
  @('crates/app-services/src/analysis_service/extraction/browser.rs', 'records', 'crates/app-services/src/analysis_service/extraction/browser/records.rs'),
  @('crates/app-services/src/analysis_service/extraction/browser.rs', 'sqlite', 'crates/app-services/src/analysis_service/extraction/browser/sqlite.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/mod.rs', 'context', 'crates/app-services/src/analysis_service/extraction/registry/context.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/mod.rs', 'dispatch', 'crates/app-services/src/analysis_service/extraction/registry/dispatch.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/mod.rs', 'extractors', 'crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/mod.rs', 'txlog', 'crates/app-services/src/analysis_service/extraction/registry/txlog.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/mod.rs', 'warnings', 'crates/app-services/src/analysis_service/extraction/registry/warnings.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs', 'amcache', 'crates/app-services/src/analysis_service/extraction/registry/extractors/amcache.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs', 'ntuser', 'crates/app-services/src/analysis_service/extraction/registry/extractors/ntuser.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs', 'sam', 'crates/app-services/src/analysis_service/extraction/registry/extractors/sam.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs', 'security', 'crates/app-services/src/analysis_service/extraction/registry/extractors/security.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs', 'software', 'crates/app-services/src/analysis_service/extraction/registry/extractors/software.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs', 'system', 'crates/app-services/src/analysis_service/extraction/registry/extractors/system.rs'),
  @('crates/app-services/src/analysis_service/extraction/registry/extractors/mod.rs', 'usrclass', 'crates/app-services/src/analysis_service/extraction/registry/extractors/usrclass.rs'),
  @('crates/app-services/src/analysis_service/extraction/summary/linux.rs', 'mapping', 'crates/app-services/src/analysis_service/extraction/summary/linux/mapping.rs'),
  @('crates/app-services/src/analysis_service/extraction/summary/registry.rs', 'structured', 'crates/app-services/src/analysis_service/extraction/summary/registry/structured.rs'),
  @('crates/app-services/src/datasource_service/lvm.rs', 'diagnostics', 'crates/app-services/src/datasource_service/lvm/diagnostics.rs'),
  @('crates/app-services/src/datasource_service/lvm.rs', 'discovery', 'crates/app-services/src/datasource_service/lvm/discovery.rs'),
  @('crates/app-services/src/datasource_service/lvm.rs', 'expansion', 'crates/app-services/src/datasource_service/lvm/expansion.rs'),
  @('crates/app-services/src/datasource_service/lvm.rs', 'model', 'crates/app-services/src/datasource_service/lvm/model.rs'),
  @('crates/app-services/src/datasource_service/lvm.rs', 'source_identity', 'crates/app-services/src/datasource_service/lvm/source_identity.rs'),
  @('crates/app-services/src/datasource_service/probe.rs', 'gpt', 'crates/app-services/src/datasource_service/probe/gpt.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'benchmark_gate', 'crates/app-services/src/governance/scoring/benchmark_gate.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'contributions', 'crates/app-services/src/governance/scoring/contributions.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'correlation_gate', 'crates/app-services/src/governance/scoring/correlation_gate.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'fixture_gate', 'crates/app-services/src/governance/scoring/fixture_gate.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'gate_status', 'crates/app-services/src/governance/scoring/gate_status.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'release_gates', 'crates/app-services/src/governance/scoring/release_gates.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'scorecard', 'crates/app-services/src/governance/scoring/scorecard.rs'),
  @('crates/app-services/src/governance/scoring.rs', 'security_gate', 'crates/app-services/src/governance/scoring/security_gate.rs'),
  @('crates/app-services/src/import_analysis/mod.rs', 'worker_coordinator', 'crates/app-services/src/import_analysis/worker_coordinator.rs'),
  @('crates/app-services/src/import_analysis/mod.rs', 'worker_model', 'crates/app-services/src/import_analysis/worker_model.rs'),
  @('crates/app-services/src/import_analysis/mod.rs', 'worker_staging', 'crates/app-services/src/import_analysis/worker_staging.rs'),
  @('crates/app-services/src/notebook_service/mod.rs', 'citation_operations', 'crates/app-services/src/notebook_service/citation_operations.rs'),
  @('crates/app-services/src/notebook_service/mod.rs', 'dto_conversion', 'crates/app-services/src/notebook_service/dto_conversion.rs'),
  @('crates/app-services/src/notebook_service/mod.rs', 'entry_operations', 'crates/app-services/src/notebook_service/entry_operations.rs'),
  @('crates/app-services/src/notebook_service/mod.rs', 'investigation_step_operations', 'crates/app-services/src/notebook_service/investigation_step_operations.rs'),
  @('crates/app-services/src/report/mod.rs', 'catalog', 'crates/app-services/src/report/catalog.rs'),
  @('crates/app-services/src/report/mod.rs', 'html_export', 'crates/app-services/src/report/html_export.rs'),
  @('crates/app-services/src/report/json.rs', 'raw_bundle', 'crates/app-services/src/report/json/raw_bundle.rs'),
  @('crates/app-services/src/report/mod.rs', 'output', 'crates/app-services/src/report/output.rs'),
  @('crates/app-services/src/report/mod.rs', 'snapshot', 'crates/app-services/src/report/snapshot.rs'),
  @('crates/app-services/src/report/mod.rs', 'types', 'crates/app-services/src/report/types.rs'),
  @('crates/app-services/src/report/mod.rs', 'warnings', 'crates/app-services/src/report/warnings.rs')
)

$parentCache = @{}
foreach ($entry in $moduleManifest) {
  $parent = $entry[0]
  $moduleName = $entry[1]
  $child = $entry[2]
  $childPath = Join-Path $repoRoot $child
  if (-not (Test-Path -LiteralPath $childPath -PathType Leaf)) {
    $errors.Add("missing Stage 4 capability module: $child")
    continue
  }
  if (-not $parentCache.ContainsKey($parent)) {
    $parentPath = Join-Path $repoRoot $parent
    if (-not (Test-Path -LiteralPath $parentPath -PathType Leaf)) {
      $errors.Add("missing Stage 4 module owner: $parent")
      continue
    }
    $parentCache[$parent] = Get-MaskedRust (Read-StrictUtf8 $parentPath)
  }
  if (-not (Test-ModuleDeclaration $parentCache[$parent] $moduleName)) {
    $errors.Add("Stage 4 capability module is not wired by its owner: $parent -> $moduleName")
  }
}

$metadata = Invoke-RustGuardCargoMetadata -RepoRoot $repoRoot
$appServicesPackage = @($metadata.packages | Where-Object { $_.name -ceq 'app-services' })
if ($appServicesPackage.Count -ne 1) {
  $errors.Add("cargo metadata must contain exactly one app-services package; found $($appServicesPackage.Count)")
} else {
  foreach ($dependency in @($appServicesPackage[0].dependencies)) {
    if (Test-ForbiddenTauriDependency ([string]$dependency.name)) {
      $alias = if ([string]::IsNullOrWhiteSpace([string]$dependency.rename)) {
        ''
      } else {
        " alias=$($dependency.rename)"
      }
      $errors.Add("app-services must not depend on Tauri runtime crates: $($dependency.name)$alias")
    }
  }
}

$serviceRoot = Join-Path $repoRoot 'crates/app-services/src'
$serviceFiles = Get-ChildItem -LiteralPath $serviceRoot -Recurse -File -Filter '*.rs'
$serviceMaskedSources = (
  $serviceFiles |
    Sort-Object -Property FullName |
    ForEach-Object { Get-MaskedRust (Read-StrictUtf8 $_.FullName) }
) -join "`n"

if ($serviceMaskedSources -cmatch '\btauri(?:::|_)|\bAppHandle\b|\bEmitter\b') {
  $errors.Add('app-services must remain independent from the Tauri runtime')
}

$moduleBaseline = Read-StrictUtf8 (
  Join-Path $repoRoot 'scripts/baselines/rust-module-size-baseline.csv'
)
if ($moduleBaseline -match '(?m)^crates/app-services/') {
  $errors.Add('Stage 4 requires zero app-services module-size baseline debt')
}

$functionBaseline = Read-StrictUtf8 (
  Join-Path $repoRoot 'scripts/baselines/rust-function-size-baseline.csv'
)
if ($functionBaseline -match '(?m)^crates/app-services/') {
  $errors.Add('Stage 4 requires zero app-services function-size baseline debt')
}

foreach ($relativePath in @(
  'crates/app-services/src/parallel_enum/coordinator.rs',
  'crates/app-services/src/parallel_enum/partition_work.rs',
  'crates/app-services/src/parallel_enum/ntfs/mft_scan.rs'
)) {
  $masked = Get-MaskedRust (Read-StrictUtf8 (Join-Path $repoRoot $relativePath))
  if ($masked -match '\bpar_iter\b|\binto_par_iter\b|\brayon::') {
    $errors.Add("evidence-image I/O module must remain serial: $relativePath")
  }
}

$importMasked = (
  Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates/app-services/src/import_pipeline') `
    -Recurse -File -Filter '*.rs' |
    ForEach-Object { Get-MaskedRust (Read-StrictUtf8 $_.FullName) }
) -join "`n"
if ($importMasked -notmatch 'pub\s+trait\s+ImportEventSink' -or
    $importMasked -notmatch 'struct\s+NoopImportEventSink') {
  $errors.Add('import_pipeline must retain its Tauri-free ImportEventSink boundary')
}

Assert-MaskedPattern `
  'crates/app-services/src/file_service/metadata/source_routing.rs' `
  '\bGlobalFileId\b' `
  'file_service source routing must resolve source-scoped file identifiers'
Assert-MaskedPattern `
  'crates/app-services/src/file_service/viewer/range/api.rs' `
  '\bMAX_RANGE_LENGTH\b' `
  'file viewer range reads must retain the bounded MAX_RANGE_LENGTH clamp'
Assert-MaskedPattern `
  'crates/app-services/src/file_service/viewer/preview_bytes.rs' `
  'length\s*\.\s*min\s*\(\s*transport::dto::MAX_VIEWER_RANGE_LENGTH\s*\)' `
  'public preview byte reads must clamp unvalidated lengths to MAX_VIEWER_RANGE_LENGTH'
Assert-MaskedPattern `
  'crates/app-services/src/file_service/viewer/media.rs' `
  '\.\s*min\s*\(\s*transport::dto::MAX_VIEWER_RANGE_LENGTH\s*\)' `
  'public media range reads must clamp unvalidated lengths to MAX_VIEWER_RANGE_LENGTH'

$artifactExtraction = Get-MaskedRust (
  Read-StrictUtf8 (
    Join-Path $repoRoot 'crates/app-services/src/artifact_service/extraction.rs'
  )
)
foreach ($pattern in @(
  'PARALLEL_EXTRACTION_BATCH_SIZE\s*:\s*usize\s*=\s*2',
  'candidates\s*\.\s*chunks\s*\(\s*PARALLEL_EXTRACTION_BATCH_SIZE\s*\)',
  'map\s*\(\s*\|file\|\s*prepare_extraction\s*\(\s*file\s*,\s*&file_reader\s*\)\s*\)',
  'prepared\s*\.\s*into_par_iter\s*\(\s*\)',
  'collect\s*::\s*<\s*Vec\s*<\s*_\s*>\s*>\s*\(\s*\)'
)) {
  if ($artifactExtraction -notmatch $pattern) {
    $errors.Add('parallel artifact extraction must use bounded serial evidence reads before CPU-only Rayon work')
    break
  }
}

Assert-MaskedPattern `
  'crates/app-services/src/artifact_service/persistence.rs' `
  'if\s+let\s+Err\s*\(\s*error\s*\)\s*=\s*populate_artifact_graph' `
  'artifact graph population must remain a non-fatal import side effect'

$correlationMasked = (
  Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates/app-services/src/correlation') `
    -Recurse -File -Filter '*.rs' |
    ForEach-Object { Get-MaskedRust (Read-StrictUtf8 $_.FullName) }
) -join "`n"
if ($correlationMasked -notmatch '\bsource_object_id\b') {
  $errors.Add('correlation must retain the sourceObjectId bridge')
}

if ($errors.Count -gt 0) {
  Write-Error "Stage 4 service boundary guard failed:`n$($errors -join "`n")"
}

Write-Host 'Stage 4 service boundary guard passed'
