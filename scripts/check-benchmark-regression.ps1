param(
  [string]$BaselinePath = '',
  [string]$OutputDir = '',
  [string]$Cargo = '',
  [int]$Runs = 3,
  [ValidateSet('small','medium','large','all')]
  [string]$FixtureLevel = 'small',
  [switch]$SkipRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')

# ── Resolve baseline path ────────────────────────────────────────
if ([string]::IsNullOrWhiteSpace($BaselinePath)) {
  $BaselinePath = Join-Path $repoRoot 'testdata/governance/v2-benchmark-baseline.json'
}
if (-not (Test-Path -LiteralPath $BaselinePath)) {
  throw "Baseline file not found: $BaselinePath"
}
$baseline = Get-Content -LiteralPath $BaselinePath -Raw -Encoding UTF8 | ConvertFrom-Json

# ── Resolve cargo ────────────────────────────────────────────────
if ([string]::IsNullOrWhiteSpace($Cargo)) {
  if ($env:CARGO) {
    $Cargo = $env.CARGO
  } else {
    $Cargo = 'cargo'
  }
}

# ── Resolve output dir ───────────────────────────────────────────
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $repoRoot 'artifacts/benchmark-regression-gates'
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

# ── Run benchmarks if not skipped ────────────────────────────────
$benchResults = $null
if (-not $SkipRun) {
  Write-Host "Running benchmark regression check (level=$FixtureLevel, runs=$Runs)"

  $runnerScript = Join-Path $repoRoot 'scripts/run-benchmark.ps1'
  if (-not (Test-Path -LiteralPath $runnerScript)) {
    throw "Benchmark runner not found: $runnerScript"
  }

  $runnerArgs = @{
    Scenario = 'all'
    Runs = $Runs
    Cold = $false
    OutputDir = $OutputDir
    Cargo = $Cargo
    FixtureLevel = $FixtureLevel
  }

  $runnerOutput = & $runnerScript @runnerArgs 2>&1 | ForEach-Object { $_.ToString() }
  $runnerOutput | ForEach-Object { Write-Host $_ }

  # Find the JSON output path from runner output
  $jsonPath = $null
  foreach ($line in $runnerOutput) {
    if ($line -match '^JSON:\s*(?<path>.+)$') {
      $jsonPath = $Matches.path.Trim()
    }
  }
  if ([string]::IsNullOrWhiteSpace($jsonPath)) {
    # Fallback: find latest benchmark JSON in output dir
    $latestJson = Get-ChildItem -LiteralPath $OutputDir -Filter "benchmark-*-*.json" |
      Sort-Object LastWriteTime -Descending |
      Select-Object -First 1
    if ($null -ne $latestJson) {
      $jsonPath = $latestJson.FullName
    }
  }
  if ([string]::IsNullOrWhiteSpace($jsonPath) -or -not (Test-Path -LiteralPath $jsonPath)) {
    throw "Benchmark runner did not produce a JSON result path."
  }

  Write-Host "Benchmark results: $jsonPath"
  $benchResults = Get-Content -LiteralPath $jsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
} else {
  Write-Host "Skipping benchmark run; checking existing baseline only."
}

# ── Validate baseline structure ──────────────────────────────────
$requiredProps = @('hostProfile', 'baselineVersion', 'lastVerifiedAt', 'requiredChecks', 'scenarios')
foreach ($prop in $requiredProps) {
  if ($null -eq $baseline.$prop) {
    throw "Baseline file is missing required property: $prop"
  }
}

if ($null -eq $baseline.requiredChecks -or $baseline.requiredChecks.Count -eq 0) {
  throw "Baseline requiredChecks is empty"
}
if ($null -eq $baseline.scenarios -or $baseline.scenarios.Count -eq 0) {
  throw "Baseline scenarios is empty"
}

# ── Verify all required checks have matching scenarios ───────────
$scenarioKeys = @{}
foreach ($s in $baseline.scenarios) {
  $key = "$($s.datasetLevel)/$($s.scenario)"
  $scenarioKeys[$key] = $s
}

$missingScenarios = @()
foreach ($check in $baseline.requiredChecks) {
  $key = "$($check.datasetLevel)/$($check.scenario)"
  if (-not $scenarioKeys.ContainsKey($key)) {
    $missingScenarios += $key
  }
}
if ($missingScenarios.Count -gt 0) {
  throw "Required checks reference scenarios not present in baseline: $($missingScenarios -join ', ')"
}

# ── If benchmark results available, check against thresholds ─────
$violations = @()
if ($null -ne $benchResults -and $null -ne $benchResults.scenarios) {
  Write-Host ''
  Write-Host '=== Regression Check ==='
  Write-Host "$('Scenario'.PadRight(22)) $('Measured'.PadLeft(10)) $('Threshold'.PadLeft(12)) $('Status'.PadLeft(8))"
  Write-Host "$('-' * 22) $('-' * 10) $('-' * 12) $('-' * 8)"

  foreach ($measured in $benchResults.scenarios) {
    $key = "$($measured.datasetLevel)/$($measured.scenario)"
    $check = $baseline.requiredChecks | Where-Object {
      "$($_.datasetLevel)/$($_.scenario)" -eq $key
    } | Select-Object -First 1

    if ($null -eq $check) {
      Write-Host "$($key.PadRight(22)) $("$($measured.p95Ms)ms".PadLeft(10)) $('N/A'.PadLeft(12)) $('NO-BASELINE'.PadLeft(8))"
      continue
    }

    $threshold = [int64]$check.thresholdP95Ms
    $measuredMs = [int64]$measured.p95Ms
    $status = if ($measuredMs -le $threshold) { 'PASS' } else { 'FAIL' }

    Write-Host "$($key.PadRight(22)) $("${measuredMs}ms".PadLeft(10)) $("${threshold}ms".PadLeft(12)) $($status.PadLeft(8))"

    if ($status -eq 'FAIL') {
      $violations += "$key" + ": measured ${measuredMs}ms > threshold ${threshold}ms"
    }
  }
}

# ── Check baseline doc references ────────────────────────────────
$baselineDocPath = Join-Path $repoRoot 'docs/benchmark-baseline.md'
if (-not (Test-Path -LiteralPath $baselineDocPath)) {
  throw "Benchmark baseline doc missing: docs/benchmark-baseline.md"
}

$readmePath = Join-Path $repoRoot 'README.md'
$readmeContent = Get-Content -LiteralPath $readmePath -Raw -Encoding UTF8
if (-not $readmeContent.Contains('docs/benchmark-baseline.md')) {
  Write-Host 'WARNING: README.md does not reference docs/benchmark-baseline.md'
}

$agentsPath = Join-Path $repoRoot 'AGENTS.md'
$agentsContent = Get-Content -LiteralPath $agentsPath -Raw -Encoding UTF8
if (-not $agentsContent.Contains('docs/benchmark-baseline.md')) {
  Write-Host 'WARNING: AGENTS.md does not reference docs/benchmark-baseline.md'
}

$docIndexPath = Join-Path $repoRoot 'docs/documentation-index.md'
$docIndexContent = Get-Content -LiteralPath $docIndexPath -Raw -Encoding UTF8
if (-not $docIndexContent.Contains('docs/benchmark-baseline.md')) {
  Write-Host 'WARNING: documentation-index.md does not reference docs/benchmark-baseline.md'
}

# ── Final verdict ────────────────────────────────────────────────
if ($violations.Count -gt 0) {
  $msg = "BENCHMARK REGRESSION DETECTED:`n  " + ($violations -join "`n  ")
  throw $msg
}

Write-Host ''
Write-Host 'Benchmark regression gate passed'
Write-Host "Baseline: $BaselinePath"
Write-Host "Level: $FixtureLevel  Scenarios checked: $($baseline.scenarios.Count)"
Write-Host "Required checks: $($baseline.requiredChecks.Count)"
