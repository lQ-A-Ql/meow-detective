param(
  [Parameter(Mandatory)][string]$ResultsPath,
  [ValidateRange(1, 100)][int]$Runs = 1,
  [ValidateSet('small', 'medium', 'large')][string]$FixtureLevel = 'small',
  [string]$Scenario = 'all'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'lib/Benchmark.Contracts.ps1')
$catalog = Get-BenchmarkCatalog $repoRoot
$selected = @(Select-BenchmarkScenarios $catalog $Scenario)
$text = Get-Content -LiteralPath $ResultsPath -Raw -Encoding UTF8
$results = $text | ConvertFrom-Json
$actual = @($results.scenarios | ForEach-Object { $_.scenario } | Sort-Object -CaseSensitive)
if (($actual -join '|') -cne (($selected | Sort-Object -CaseSensitive) -join '|')) {
  throw 'The actual result file has an unexpected scenario set'
}
foreach ($item in $results.scenarios) {
  Assert-BenchmarkScenario -Output ([pscustomobject]@{
    schemaVersion = $results.schemaVersion; benchmarkVersion = $results.benchmarkVersion;
    buildProfile = $results.buildProfile; scenarios = @($item)
  }) -Name $item.scenario -Runs $Runs -WarmupRuns $results.warmupRuns `
    -FixtureLevel $FixtureLevel -BuildProfile $results.buildProfile -Catalog $catalog
}

function Assert-Rejected {
  param([scriptblock]$Mutation)
  $copy = $text | ConvertFrom-Json
  & $Mutation $copy.scenarios[0]
  $rejected = $false
  try {
    Assert-BenchmarkScenario -Output ([pscustomobject]@{
      schemaVersion = $copy.schemaVersion; benchmarkVersion = $copy.benchmarkVersion;
      buildProfile = $copy.buildProfile; scenarios = @($copy.scenarios[0])
    }) -Name $results.scenarios[0].scenario -Runs $Runs -WarmupRuns $results.warmupRuns `
      -FixtureLevel $FixtureLevel -BuildProfile $results.buildProfile -Catalog $catalog
  } catch { $rejected = $true }
  if (-not $rejected) { throw 'A malformed derivative of the actual result was accepted' }
}

Assert-Rejected { param($item) $item.runs = @() }
Assert-Rejected { param($item) $item.runs[0].counters.PSObject.Properties.Remove('bytesRead') }
Assert-Rejected { param($item)
  if ('sources' -in $item.workload.PSObject.Properties.Name) { $item.workload.sources = 0 }
  elseif ('payloadRecords' -in $item.workload.PSObject.Properties.Name) { $item.workload.payloadRecords = 0 }
  else { $item.workload.records = 0 }
}
Write-Host "Benchmark result validation self-test passed against actual results: $ResultsPath"
