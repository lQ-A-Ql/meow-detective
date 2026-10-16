Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Import-Module (Join-Path $root 'scripts/lib/ImportBenchmark.Input.psm1') -Force
Import-Module (Join-Path $root 'scripts/lib/ImportBenchmark.Report.psm1') -Force

function Assert-Equal($Actual, $Expected, [string]$Message) {
  if ($Actual -cne $Expected) { throw "$Message (actual=$Actual, expected=$Expected)" }
}
function Assert-Throws([scriptblock]$Action, [string]$Message) {
  $threw = $false
  try { & $Action } catch { $threw = $true }
  if (-not $threw) { throw "Expected rejection: $Message" }
}
function New-Payload {
  # A partition duration is intentionally larger than source wall time: adding
  # overlapping worker durations must never inflate the workflow statistic.
  [pscustomobject]@{
    schemaVersion = 2; oracleVersion = 2; oracleOrder = 'canonical-primary-id; graph-nodes-before-edges'
    analysisMode = 'metadataOnly'; platform = 'windows'; workersRequested = 4; workersResolved = 2
    catalogRows = 9; graphRows = 60; timelineRows = 27
    catalogSha256 = ('a' * 64); graphSha256 = ('b' * 64); timelineSha256 = ('c' * 64)
    report = [pscustomobject]@{
      schemaVersion = 1; processPeakRssMb = 123
      samples = @([pscustomobject]@{ phase = 'workflow'; state = 'completed'; inclusive = $true; elapsedNs = 2e9 },
        [pscustomobject]@{ phase = 'mftRead'; state = 'completed'; partitionIndex = 0; elapsedNs = 3e9 })
      partitionAggregates = @([pscustomobject]@{ phase = 'mftRead'; scannedRecords = 100; elapsedNs = 3e9 })
    }
  }
}

$stats = Get-ImportBenchmarkStatistics -Values @(9, 1, 5, 3)
Assert-Equal $stats.median 4 'Median of even sample'
Assert-Equal $stats.p95 9 'Nearest rank p95'
Assert-Equal (Get-ImportBenchmarkStatistics -Values @(5)).median 5 'Single trial'
Assert-Throws { Get-ImportBenchmarkStatistics -Values @() } 'Empty measurements'
Assert-Throws { Get-ImportBenchmarkStatistics -Values @([double]::NaN) } 'NaN measurements'

$payload = New-Payload
Assert-ImportBenchmarkPayload -Payload $payload -Workers 4
$changed = New-Payload
$changed.timelineRows++
Assert-Throws { Assert-ImportBenchmarkOracle $payload $changed } 'Lost timeline row'
$changed = New-Payload
$changed.graphSha256 = 'd' * 64
Assert-Throws { Assert-ImportBenchmarkOracle $payload $changed } 'Changed graph content'
$changed = New-Payload
$changed.oracleVersion = 1
Assert-Throws { Assert-ImportBenchmarkPayload $changed 4 } 'Old oracle version'
Assert-Throws { Assert-ImportBenchmarkPayload $payload 1 } 'Wrong worker condition'
$changed = New-Payload
$changed.report.samples[0].state = 'failed'
Assert-Throws { Assert-ImportBenchmarkPayload $changed 4 } 'Failed workflow'
$trial = [pscustomobject]@{ condition = 'raw-w4'; input = 0; payload = $payload; process = [pscustomobject]@{ sampledPeakHandles = 40 } }
$summary = @(Get-ImportBenchmarkSummary -Results @($trial))
Assert-Equal $summary.Count 1 'One condition'
Assert-Equal $summary[0].workflowSeconds.median 2 'Workflow remains source wall time'
Assert-Equal ($summary[0].phases | Where-Object { $_.scope -eq 'partition-work-sum' }).seconds.median 3 'Partition work retains own scope'
Assert-Equal @($summary[0].phases | Where-Object { $_.phase -eq 'mftRead' -and $_.scope -eq 'source-wall-inclusive' }).Count 0 'Partition sample is not source wall time'
Assert-Equal @($summary[0].processMetrics | Where-Object { $_.availableForAllTrials }).Count 0 'Missing process counters are unavailable, not zero'
$payload.report | Add-Member -NotePropertyName processUsage -NotePropertyValue ([pscustomobject]@{
  scope = 'workflowProcessWindow'; wallTimeNs = 2e9; userCpuNs = 3e9; kernelCpuNs = 1e9
  cpuCoreEquivalents = 2; processReadTransferBytes = 0; handlesAtStart = 40; handlesAtEnd = 41
})
$resources = @(Get-ImportBenchmarkSummary -Results @($trial))[0].processMetrics
Assert-Equal ($resources | Where-Object { $_.metric -eq 'userCpuSeconds' }).statistics.median 3 'CPU uses seconds'
Assert-Equal ($resources | Where-Object { $_.metric -eq 'cpuCoreEquivalents' }).statistics.median 2 'CPU core equivalents are not percentages'
Assert-Equal ($resources | Where-Object { $_.metric -eq 'processReadTransferBytes' }).statistics.median 0 'Supported zero IO remains zero'
Assert-Equal ($resources | Where-Object { $_.metric -eq 'processWriteTransferBytes' }).observations 0 'Unavailable IO is never synthesized'
$payload.report.partitionAggregates[0] | Add-Member -NotePropertyName maxBufferedBatches -NotePropertyValue 4
$payload.report.partitionAggregates[0] | Add-Member -NotePropertyName mainTableUpdatedRows -NotePropertyValue 82
$payload.report.partitionAggregates[0] | Add-Member -NotePropertyName sampledWalMaxBytes -NotePropertyValue 0
$counters = (@(Get-ImportBenchmarkSummary -Results @($trial))[0].phases |
  Where-Object { $_.phase -eq 'mftRead' -and $_.scope -eq 'partition-work-sum' }).counters
Assert-Equal ($counters | Where-Object { $_.metric -eq 'maxBufferedBatches' }).statistics.median 4 'Payload high water survives aggregation'
Assert-Equal ($counters | Where-Object { $_.metric -eq 'mainTableUpdatedRows' }).partitionAggregation 'sum' 'Rows are counts rather than gauges'
Assert-Equal ($counters | Where-Object { $_.metric -eq 'sampledWalMaxBytes' }).statistics.median 0 'Existing empty WAL is measured zero'

$directory = Join-Path $root ('target/import-benchmark-test-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $directory
$first = Join-Path $directory 'evidence.E01'
$second = Join-Path $directory 'evidence.E02'
$third = Join-Path $directory 'evidence.E03'
try {
  [IO.File]::WriteAllBytes($first, [byte[]]@(1, 2, 3))
  [IO.File]::WriteAllBytes($second, [byte[]]@(4, 5, 6))
  $manifest = Get-ImportBenchmarkManifest $first
  Assert-Equal $manifest.segments.Count 2 'Complete segment manifest'
  [IO.File]::WriteAllBytes($second, [byte[]]@(4, 7, 6))
  $after = Get-ImportBenchmarkManifest $first
  if ($manifest.manifestSha256 -ceq $after.manifestSha256) { throw 'Second segment mutation was missed' }
  Remove-Item -LiteralPath $second
  [IO.File]::WriteAllBytes($third, [byte[]]@(8))
  Assert-Throws { Get-ImportBenchmarkManifest $first } 'Missing middle segment'
  Assert-Throws { Get-ImportBenchmarkManifest $third } 'Noninitial segment'
} finally {
  foreach ($path in @($first, $second, $third)) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
  }
  Remove-Item -LiteralPath $directory
}
Write-Host 'Import benchmark contract tests passed'
