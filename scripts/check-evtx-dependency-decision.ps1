$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$denyPath = Join-Path $repoRoot "deny.toml"
$decisionPath = Join-Path $repoRoot "docs/evtx-dependency-decision.md"
$cargoPath = Join-Path $repoRoot "Cargo.toml"
$lockPath = Join-Path $repoRoot "Cargo.lock"
$patchedManifestPath = Join-Path $repoRoot "crates/evtx-patched/Cargo.toml"
$patchedErrorPath = Join-Path $repoRoot "crates/evtx-patched/src/err.rs"
$patchedParserPath = Join-Path $repoRoot "crates/evtx-patched/src/evtx_parser.rs"
$patchedRegressionPath = Join-Path $repoRoot "crates/evtx-patched/tests/serialized_records.rs"

if (-not (Test-Path -LiteralPath $denyPath)) {
  throw "deny.toml not found"
}
if (-not (Test-Path -LiteralPath $decisionPath)) {
  throw "EVTX dependency decision document is missing"
}
if (-not (Test-Path -LiteralPath $cargoPath)) {
  throw "Cargo.toml not found"
}
if (-not (Test-Path -LiteralPath $lockPath)) {
  throw "Cargo.lock not found"
}
if (-not (Test-Path -LiteralPath $patchedManifestPath)) {
  throw "patched EVTX manifest is missing"
}
foreach ($requiredPath in @($patchedErrorPath, $patchedParserPath, $patchedRegressionPath)) {
  if (-not (Test-Path -LiteralPath $requiredPath)) {
    throw "patched EVTX cancellation/chunk-identity source is missing: $requiredPath"
  }
}

$deny = Get-Content -LiteralPath $denyPath -Raw -Encoding UTF8
$decision = Get-Content -LiteralPath $decisionPath -Raw -Encoding UTF8
$cargo = Get-Content -LiteralPath $cargoPath -Raw -Encoding UTF8
$lock = Get-Content -LiteralPath $lockPath -Raw -Encoding UTF8
$patchedManifest = Get-Content -LiteralPath $patchedManifestPath -Raw -Encoding UTF8
$patchedError = Get-Content -LiteralPath $patchedErrorPath -Raw -Encoding UTF8
$patchedParser = Get-Content -LiteralPath $patchedParserPath -Raw -Encoding UTF8
$patchedRegression = Get-Content -LiteralPath $patchedRegressionPath -Raw -Encoding UTF8

if ($deny -match 'RUSTSEC-2021-0153') {
  throw "RUSTSEC-2021-0153 should not be ignored after the local EVTX patch"
}
if ($cargo -notmatch 'evtx\s*=\s*\{\s*path\s*=\s*"crates/evtx-patched"') {
  throw "workspace evtx dependency must point at crates/evtx-patched"
}
if ($cargo -notmatch 'evtx\s*=.*features\s*=\s*\[\s*"multithreading"\s*\]') {
  throw "workspace evtx dependency must enable bounded chunk-parsing parallelism"
}
# encoding_rs may be pinned inline (legacy) or centralized via
# `encoding_rs.workspace = true` per the workspace-dependency convention
# (CLAUDE.md "Naming conventions"); accept either form, but require the
# workspace root to pin a concrete 0.8.x version when centralized.
$usesWorkspaceDep = $patchedManifest -match 'encoding_rs\.workspace\s*=\s*true'
$usesInlineDep = $patchedManifest -match 'encoding_rs\s*=\s*"0\.8"'
if (-not $usesWorkspaceDep -and -not $usesInlineDep) {
  throw "patched EVTX manifest must depend on encoding_rs"
}
if ($usesWorkspaceDep -and ($cargo -notmatch 'encoding_rs\s*=\s*"0\.8"')) {
  throw "root Cargo.toml [workspace.dependencies] must pin encoding_rs to 0.8.x"
}
if ($patchedManifest -match '\[dependencies\.encoding\]' -or $patchedManifest -match 'encoding\s*=\s*"0\.2\.33"') {
  throw "patched EVTX manifest must not depend on encoding 0.2.33"
}
if ($lock -match 'name = "encoding"') {
  throw "Cargo.lock still contains the unmaintained encoding crate"
}
if ($patchedError -notmatch 'FailedToReadChunk\(io::Error\)') {
  throw "patched EVTX errors must preserve chunk read failures"
}
if ($patchedParser -notmatch 'ChunkError::FailedToReadChunk\(error\)') {
  throw "patched EVTX parser must propagate the original chunk read error"
}
if ($patchedParser -match 'read_to_end\s*\(\s*&mut\s+chunk_data') {
  throw "patched EVTX chunk loading must not reintroduce read_to_end Interrupted retries"
}
if ($patchedRegression -notmatch 'multibatch_parse_error_uses_absolute_chunk_identity') {
  throw "patched EVTX fork must retain the absolute chunk identity regression"
}

foreach ($needle in @(
  'crates/evtx-patched',
  'encoding_rs',
  'multithreading',
  'FailedToReadChunk',
  'Interrupted',
  '256 records',
  'RUSTSEC-2021-0153',
  'Resolved Decision'
)) {
  if (-not $decision.Contains($needle)) {
    throw "EVTX dependency decision is missing: $needle"
  }
}

Write-Host "EVTX dependency decision guard passed: dependency, cancellation, and chunk identity patches are locked"
