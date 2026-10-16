Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$contract = Get-Content (Join-Path $root 'scripts/baselines/import-optimization-contracts.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($contract.schemaVersion -ne 1 -or $contract.oracleVersion -ne 2) { throw 'Unsupported frozen oracle contract' }
$queries = Get-Content (Join-Path $root $contract.querySource) -Raw -Encoding UTF8
foreach ($table in $contract.tables) {
  $pattern = '\b' + [regex]::Escape($table.query) + ':\s*&str\s*=\s*"SELECT\s+([^"]+)\s+FROM\s+' + [regex]::Escape($table.table) + '"'
  $match = [regex]::Match($queries, $pattern)
  if (-not $match.Success) { throw "Frozen query not found: $($table.query)" }
  $actual = @($match.Groups[1].Value.Split(',') | ForEach-Object { $_.Trim() })
  if (($actual -join ',') -cne ($table.columns -join ',')) { throw "Oracle columns drifted: $($table.query)" }
}
foreach ($invariant in $contract.invariants) {
  foreach ($property in @('code', 'tests')) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $invariant.$property))) { throw "Missing $property for invariant $($invariant.id)" }
  }
}
Write-Host "Frozen oracle contracts passed: $($contract.tables.Count) table queries, $($contract.invariants.Count) invariants"
$sidecar=Get-Content (Join-Path $root 'scripts/baselines/import-optimization-semantic-oracle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($sidecar.schemaVersion -ne 1 -or $sidecar.sidecarVersion -ne 1 -or $sidecar.legacyOracleVersion -ne 2 -or
    ($sidecar.analysisModes -join ',') -cne 'metadataOnly,fullContent' -or $sidecar.byteOracles.Count -ne 5) { throw 'Semantic sidecar contract drifted' }
Write-Host 'Semantic sidecar contract passed: version 1, legacy oracle 2 preserved'
