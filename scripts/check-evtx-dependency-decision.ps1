$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$denyPath = Join-Path $repoRoot "deny.toml"
$decisionPath = Join-Path $repoRoot "docs/evtx-dependency-decision.md"

if (-not (Test-Path -LiteralPath $denyPath)) {
  throw "deny.toml not found"
}
if (-not (Test-Path -LiteralPath $decisionPath)) {
  throw "EVTX dependency decision document is missing"
}

$deny = Get-Content -LiteralPath $denyPath -Raw -Encoding UTF8
$decision = Get-Content -LiteralPath $decisionPath -Raw -Encoding UTF8

if ($deny -notmatch 'RUSTSEC-2021-0153') {
  throw "deny.toml no longer tracks RUSTSEC-2021-0153; update docs/evtx-dependency-decision.md"
}
if ($deny -notmatch 'evtx 0\.11\.2 pulls encoding') {
  throw "RUSTSEC-2021-0153 exception no longer documents evtx -> encoding"
}
if ($deny -notmatch 'expires:\s*2026-09-01') {
  throw "RUSTSEC-2021-0153 exception expiry changed; update docs/evtx-dependency-decision.md"
}

foreach ($needle in @(
  'RUSTSEC-2021-0153',
  'encoding = 0.2.33',
  '2026-09-01',
  'evtx = 0.11.2',
  'Required Follow-Up Before Expiry'
)) {
  if (-not $decision.Contains($needle)) {
    throw "EVTX dependency decision is missing: $needle"
  }
}

Write-Host "EVTX dependency decision guard passed"
