param(
  [string]$Scenario = 'all',
  [ValidateRange(1, 100)][int]$Runs = 5,
  [ValidateRange(0, 10)][int]$WarmupRuns = 2,
  [string]$OutputDir = '',
  [string]$Cargo = '',
  [ValidateSet('small', 'medium', 'large')][string]$FixtureLevel = 'small',
  [ValidateSet('debug', 'release')][string]$BuildProfile = 'debug'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'lib/Benchmark.Contracts.ps1')
. (Join-Path $PSScriptRoot 'lib/Benchmark.Process.ps1')
$catalog = Get-BenchmarkCatalog $repoRoot
$thresholdPath = Join-Path $repoRoot 'testdata/governance/v2-benchmark-baseline.json'
$thresholdPolicy = Get-Content -LiteralPath $thresholdPath -Raw -Encoding UTF8 | ConvertFrom-Json
$selected = @(Select-BenchmarkScenarios $catalog $Scenario)
if ([string]::IsNullOrWhiteSpace($Cargo)) {
  $Cargo = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $repoRoot 'output/performance-benchmarks'
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$scratch = Join-Path $repoRoot 'output/performance-implementation-2026-11-13/benchmark/scratch'
New-Item -ItemType Directory -Path $scratch -Force | Out-Null
$timestamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fffffff')
$scenarios = [System.Collections.Generic.List[object]]::new()
$hostMetadata = Get-BenchmarkHostMetadata $repoRoot

foreach ($name in $selected) {
  Write-Host "Benchmark: $name level=$FixtureLevel runs=$Runs warmups=$WarmupRuns profile=$BuildProfile"
  $raw = Invoke-BenchmarkProcess -Repository $repoRoot -Cargo $Cargo -Scenario $name `
    -Runs $Runs -WarmupRuns $WarmupRuns -FixtureLevel $FixtureLevel -BuildProfile $BuildProfile `
    -Scratch $scratch -LogPath (Join-Path $OutputDir "$timestamp-$name-raw.log")
  Assert-BenchmarkScenario -Output $raw -Name $name -Runs $Runs `
    -WarmupRuns $WarmupRuns -FixtureLevel $FixtureLevel -BuildProfile $BuildProfile -Catalog $catalog
  $scenarios.Add($raw.scenarios[0])
}

$output = [ordered]@{
  schemaVersion = 2
  benchmarkVersion = $catalog.harnessVersion
  baselineVersion = $thresholdPolicy.baselineVersion
  baselinePolicySha256 = (Get-FileHash -LiteralPath $thresholdPath -Algorithm SHA256).Hash.ToLowerInvariant()
  generatedAt = [DateTime]::UtcNow.ToString('o')
  lastVerifiedAt = [DateTime]::UtcNow.ToString('o')
  hostProfile = $hostMetadata.display
  host = $hostMetadata
  fixtureLevel = $FixtureLevel
  buildProfile = $BuildProfile
  processIsolation = 'one-process-per-scenario'
  cachePolicy = 'fresh-process-with-explicit-warmups'
  runs = $Runs
  warmupRuns = $WarmupRuns
  selectedScenarios = $selected
  scenarios = @($scenarios.ToArray())
}
$jsonPath = Join-Path $OutputDir "benchmark-$FixtureLevel-$timestamp.json"
$output | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $jsonPath -Encoding UTF8
foreach ($item in $scenarios) {
  Write-Host "$($item.scenario): p95=$($item.p95Ms) ms, sampled RSS=$($item.memoryPeakMb) MiB"
}
Write-Host "JSON: $jsonPath"
