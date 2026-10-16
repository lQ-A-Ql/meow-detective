Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Import-Module (Join-Path $root 'scripts/lib/ImportBenchmark.Protocol.psm1') -Force
$protocolPath = Join-Path $root 'scripts/baselines/import-optimization-protocol.json'
$protocol = Read-ImportBenchmarkProtocol -Path $protocolPath
$checks = 0
function Assert-True([bool]$Value, [string]$Message) {
  if (-not $Value) { throw $Message }; $script:checks++
}
function Assert-Throws([scriptblock]$Action, [string]$Message) {
  $threw = $false
  try { & $Action | Out-Null } catch { $threw = $true }
  Assert-True $threw "Expected rejection: $Message"
}
function New-Trial([bool]$Warmup = $false, [int]$Round = 0) {
  if ($Round -eq 0) { $Round = if ($Warmup) { 1 } else { 6 } }
  [pscustomobject]@{ condition = 'raw-w4'; input = 0; warmup = $Warmup; round = $Round; sequence = $Round
    payload = [pscustomobject]@{
      schemaVersion = 2; oracleVersion = 2; oracleOrder = 'canonical-primary-id; graph-nodes-before-edges'
      analysisMode = 'metadataOnly'; platform = 'windows'; workersRequested = 4; workersResolved = 4
      catalogRows = 9; graphRows = 60; timelineRows = 27
      catalogSha256 = ('a' * 64); graphSha256 = ('b' * 64); timelineSha256 = ('c' * 64)
      report = [pscustomobject]@{ schemaVersion = 1; processPeakRssMb = 2048
        samples = @([pscustomobject]@{ phase = 'workflow'; state = 'completed'; inclusive = $true; elapsedNs = 1e9 })
        partitionAggregates = @([pscustomobject]@{ phase = 'mftRead'; state = 'completed'; scannedRecords = 9 }) }
    }
  }
}
$conditions = @([pscustomobject]@{ key = 'raw-w4'; input = 0; workers = 4 })
function Assess($Results, $Warmups, [string]$Kind = 'Formal') {
  Get-ImportBenchmarkRunAssessment -Protocol $protocol -RunKind $Kind -Conditions $conditions `
    -Results $Results -Warmups $Warmups -Runs 5 -WarmupRuns 1 -Release $true
}
$results = @(1..5 | ForEach-Object { New-Trial -Round ($_ + 1) })
$warmups = @(New-Trial $true)
$valid = Assess $results $warmups
Assert-True $valid.measurementGatesPassed 'Complete formal measurement should pass'
Assert-True $valid.formalMeasurement 'Complete five-round Release measurement should be marked formal'
Assert-True ($valid.performanceVerdict -eq 'not-evaluated-without-baseline') 'A single version cannot prove a speedup'
Assert-True ($valid.coverage -eq 'selected-inputs-only') 'Selected conditions cannot imply full release coverage'
Assert-True (-not (Assess $results $warmups 'Exploratory').formalMeasurement) 'Exploratory data must not become formal automatically'
Assert-True (-not (Assess @($results[0..3]) $warmups).measurementGatesPassed) 'A missing formal trial must fail'
Assert-True (-not (Assess @($results[0..3] + $results[0]) $warmups).measurementGatesPassed) 'Duplicate trial cannot replace a missing round'
Assert-True (-not (Assess $results @()).measurementGatesPassed) 'Missing warmup must fail'
$extra = New-Trial; $extra.condition = 'unknown'
Assert-True (-not (Assess @($results + $extra) $warmups).measurementGatesPassed) 'Unplanned input condition must fail'
$changed = New-Trial; $changed.warmup = $true
Assert-True (-not (Assess @($results[0..3] + $changed) $warmups).measurementGatesPassed) 'Warmup must not count as a measured trial'
$changed = New-Trial; $changed.input = 1
Assert-True (-not (Assess @($results[0..3] + $changed) $warmups).measurementGatesPassed) 'Input scope mismatch must fail'
$changed = New-Trial; $changed.payload.timelineSha256 = ('d' * 64)
Assert-True (-not (Assess @($results[0..3] + $changed) $warmups).measurementGatesPassed) 'Same-input oracle divergence must fail'
$changed = New-Trial; $changed.payload.workersRequested = 1
Assert-True (-not (Assess @($results[0..3] + $changed) $warmups).measurementGatesPassed) 'Worker budget mismatch must fail'
$changed = New-Trial; $changed.payload.report.partitionAggregates[0].state = 'failed'
Assert-True (-not (Assess @($results[0..3] + $changed) $warmups).measurementGatesPassed) 'Failed partition work must fail'
foreach ($peak in @(2049, 0, -1, [double]::NaN, [double]::PositiveInfinity, 'unknown', $true, $null)) {
  $changed = New-Trial; $changed.payload.report.processPeakRssMb = $peak
  Assert-True (-not (Assess @($results[0..3] + $changed) $warmups).measurementGatesPassed) 'Invalid or excessive measured RSS must fail'
}
$changed = New-Trial $true; $changed.payload.report.processPeakRssMb = 2049
Assert-True (-not (Assess $results @($changed)).measurementGatesPassed) 'Warmup memory violation must fail too'
Assert-Throws { Assert-ImportBenchmarkRunConfiguration $protocol 'Formal' 4 1 $true } 'Four formal trials'
Assert-Throws { Assert-ImportBenchmarkRunConfiguration $protocol 'Formal' 5 0 $true } 'Formal without warmup'
Assert-Throws { Assert-ImportBenchmarkRunConfiguration $protocol 'Formal' 5 1 $false } 'Formal debug build'
Assert-Throws { Assert-ImportBenchmarkRunConfiguration $protocol 'Pressure' 5 0 $true } 'Pressure is a single-trial observation'
Assert-Throws { Assert-ImportBenchmarkRunConfiguration $protocol 'Pressure' 1 0 $false } 'Pressure debug build'
Assert-ImportBenchmarkRunConfiguration $protocol 'Pressure' 1 0 $true

$directory = Join-Path $root ('target/import-protocol-test-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $directory
$temporary = Join-Path $directory 'protocol.json'
try {
  foreach ($mutation in @(
      { param($p) $p.schemaVersion = 99 },
      { param($p) $p.gates.maxImportProcessPeakRssMiB = 4096 },
      { param($p) $p.gates.maxImportProcessPeakRssMiB = $true },
      { param($p) $p.measurement.minimumFormalRuns = 1 },
      { param($p) $p.measurement.minimumWarmupRuns = 0 },
      { param($p) $p.gates.maximumOtherWorkflowRegressionPercent = 10 },
      { param($p) $p.oracle.oracleVersion = 1 }
    )) {
    $copy = Get-Content -LiteralPath $protocolPath -Raw -Encoding UTF8 | ConvertFrom-Json
    & $mutation $copy
    [IO.File]::WriteAllText($temporary, ($copy | ConvertTo-Json -Depth 12), (New-Object Text.UTF8Encoding($false)))
    Assert-Throws { Read-ImportBenchmarkProtocol -Path $temporary } 'Invalid or weakened protocol'
  }
  [IO.File]::WriteAllText($temporary, '{invalid json', (New-Object Text.UTF8Encoding($false)))
  Assert-Throws { Read-ImportBenchmarkProtocol -Path $temporary } 'Malformed protocol JSON'
  $copy = Get-Content -LiteralPath $protocolPath -Raw -Encoding UTF8 | ConvertFrom-Json
  [IO.File]::WriteAllText($temporary, ($copy | ConvertTo-Json -Depth 12), (New-Object Text.UTF8Encoding($false)))
  $before = Read-ImportBenchmarkProtocol -Path $temporary
  [IO.File]::AppendAllText($temporary, "`n")
  $after = Read-ImportBenchmarkProtocol -Path $temporary
  Assert-True ($before.sha256 -cne $after.sha256) 'Protocol hash must detect even formatting changes'
} finally {
  if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
  Remove-Item -LiteralPath $directory
}
$runner = Join-Path $root 'scripts/run-import-benchmark.ps1'
Assert-Throws { & $runner -FixturePath '' } 'Explicit empty fixture cannot switch to synthetic input'
Assert-Throws { & $runner -FixturePath '   ' } 'Whitespace fixture cannot switch to synthetic input'
Assert-Throws { & $runner -FixturePath $root } 'Directory fixture must fail before build'
Assert-Throws { & $runner -RunKind Formal -Runs 1 -WarmupRuns 0 -Release } 'Formal CLI must fail before build'
Import-Module (Join-Path $root 'scripts/lib/ImportBenchmark.Input.psm1') -Force
$code = Get-ImportBenchmarkCodeContext -RepoRoot $root
Assert-True (@($code.files | Where-Object { $_.path -eq 'scripts/baselines/import-optimization-protocol.json' }).Count -eq 1) 'Protocol belongs to the versioned code-input hash'
Write-Host "Import benchmark protocol tests passed: $checks assertions"
