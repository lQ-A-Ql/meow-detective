param(
  [string]$BaselinePath = '',
  [string]$OutputDir = '',
  [string]$Cargo = '',
  [ValidateRange(1, 100)][int]$Runs = 3,
  [ValidateSet('small', 'medium', 'large')][string]$FixtureLevel = 'small',
  [string]$Scenario = 'all',
  [string]$ResultsPath = '',
  [switch]$StructureOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'lib/Benchmark.Contracts.ps1')
. (Join-Path $PSScriptRoot 'lib/Benchmark.Thresholds.ps1')
$catalog = Get-BenchmarkCatalog $repoRoot
$selected = @(Select-BenchmarkScenarios $catalog $Scenario)
if ([string]::IsNullOrWhiteSpace($BaselinePath)) {
  $BaselinePath = Join-Path $repoRoot 'testdata/governance/v2-benchmark-baseline.json'
}
$baseline = Get-Content -LiteralPath $BaselinePath -Raw -Encoding UTF8 | ConvertFrom-Json
$required = @($baseline.requiredChecks)
if ($required.Count -ne 18) { throw 'The core benchmark policy requires 18 latency checks' }
$expectedKeys = @(foreach ($level in @('small', 'medium', 'large')) {
  foreach ($name in $catalog.smoke) { "$level/$name" }
}) | Sort-Object -CaseSensitive
$actualKeys = @($required | ForEach-Object { "$($_.datasetLevel)/$($_.scenario)" }) |
  Sort-Object -CaseSensitive
if (@($actualKeys | Sort-Object -Unique -CaseSensitive).Count -ne $actualKeys.Count -or
    ($actualKeys -join '|') -cne ($expectedKeys -join '|')) {
  throw 'Core latency checks must uniquely cover all three levels and all six smoke scenarios'
}
foreach ($check in $required) {
  $matches = @($baseline.scenarios | Where-Object {
    $_.datasetLevel -eq $check.datasetLevel -and $_.scenario -eq $check.scenario
  })
  if ($matches.Count -ne 1 -or $check.scenario -notin $catalog.smoke) {
    throw "Baseline scenario coverage is invalid: $($check.datasetLevel)/$($check.scenario)"
  }
}
if ($StructureOnly) {
  if ($ResultsPath) { throw 'StructureOnly cannot be combined with ResultsPath' }
  Write-Host "Benchmark structure passed: $($catalog.scenarioNames.Count) scenarios, $($required.Count) core latency checks"
  return
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $repoRoot 'output/performance-regression'
}
if ([string]::IsNullOrWhiteSpace($ResultsPath)) {
  $arguments = @{ Scenario = $Scenario; Runs = $Runs; WarmupRuns = 0;
    FixtureLevel = $FixtureLevel; OutputDir = $OutputDir; Cargo = $Cargo }
  $runnerLines = @(& (Join-Path $PSScriptRoot 'run-benchmark.ps1') @arguments 6>&1 |
    ForEach-Object { $_.ToString() })
  $runnerLines | ForEach-Object { Write-Host $_ }
  $paths = @($runnerLines | Where-Object { $_.StartsWith('JSON: ') })
  if ($paths.Count -ne 1) { throw 'Runner did not return exactly one result path' }
  $ResultsPath = $paths[0].Substring('JSON: '.Length)
}
$results = Get-Content -LiteralPath $ResultsPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($results.schemaVersion -ne 2) { throw 'Unsupported benchmark result schema' }
$actual = @($results.scenarios | ForEach-Object { $_.scenario } | Sort-Object)
$expected = @($selected | Sort-Object)
if (($actual -join '|') -ne ($expected -join '|')) { throw 'Measured scenario set is incomplete or duplicated' }
foreach ($item in $results.scenarios) {
  Assert-BenchmarkScenario -Output ([pscustomobject]@{ schemaVersion = 2;
    benchmarkVersion = $results.benchmarkVersion;
    buildProfile = $results.buildProfile; scenarios = @($item) }) -Name $item.scenario `
    -Runs $Runs -WarmupRuns $results.warmupRuns -FixtureLevel $FixtureLevel -BuildProfile $results.buildProfile -Catalog $catalog
  Test-BenchmarkStructure $item
  $check = @($required | Where-Object {
    $_.datasetLevel -eq $FixtureLevel -and $_.scenario -eq $item.scenario
  })
  if ($check.Count -eq 1 -and $item.p95Ms -gt $check[0].thresholdP95Ms) {
    throw "Latency threshold exceeded: $($item.scenario) $($item.p95Ms) ms > $($check[0].thresholdP95Ms) ms"
  }
}
Write-Host "Benchmark runtime passed: $($actual.Count) scenarios; evidence: $ResultsPath"
