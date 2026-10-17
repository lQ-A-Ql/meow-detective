Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-ImportBenchmarkPayload {
  param($Payload, [int]$Workers)
  if ($Payload.schemaVersion -ne 2 -or $Payload.oracleVersion -ne 2 -or $Payload.report.schemaVersion -ne 1 -or
      $Payload.oracleOrder -ne 'canonical-primary-id; graph-nodes-before-edges' -or
      $Payload.analysisMode -notin @('metadataOnly','fullContent') -or $Payload.platform -ne 'windows') { throw 'Incompatible benchmark contract' }
  if ($Payload.workersRequested -ne $Workers -or $Payload.workersResolved -lt 1 -or $Payload.workersResolved -gt $Workers) {
    throw 'Worker budget does not match the requested condition'
  }
  foreach ($kind in @('catalog', 'graph', 'timeline')) {
    if ($Payload.("${kind}Rows") -le 0 -or $Payload.("${kind}Sha256") -notmatch '^[0-9a-f]{64}$') { throw "Invalid $kind oracle" }
  }
  $workflow = @($Payload.report.samples | Where-Object { $_.phase -eq 'workflow' })
  if ($workflow.Count -ne 1 -or $workflow[0].state -ne 'completed' -or $workflow[0].elapsedNs -le 0) {
    throw 'A complete workflow wall time is required'
  }
  if (@($Payload.report.samples | Where-Object { $_.state -eq 'failed' -or $_.state -eq 'cancelled' }).Count -gt 0) {
    throw 'Import reported failed or cancelled phase samples'
  }
  $read = @($Payload.report.partitionAggregates | Where-Object { $_.phase -eq 'mftRead' })
  if ($read.Count -ne 1 -or $read[0].scannedRecords -le 0) { throw 'Benchmark requires NTFS MFT work' }
}

function Assert-ImportBenchmarkOracle {
  param($Expected, $Actual)
  foreach ($key in @('oracleVersion', 'oracleOrder', 'analysisMode', 'platform',
      'catalogRows', 'catalogSha256', 'graphRows', 'graphSha256', 'timelineRows', 'timelineSha256')) {
    if ($Expected.$key -cne $Actual.$key) { throw "Correctness oracle differs: $key" }
  }
  if ($Expected.PSObject.Properties['semanticOracle'] -and $Actual.PSObject.Properties['semanticOracle']) {
    $left = $Expected.semanticOracle | ConvertTo-Json -Depth 25 -Compress
    $right = $Actual.semanticOracle | ConvertTo-Json -Depth 25 -Compress
    if ($left -cne $right) { throw 'Versioned semantic oracle differs' }
  }
}

function Get-ImportBenchmarkStatistics {
  param([double[]]$Values)
  if ($Values.Count -eq 0) { throw 'Cannot summarize an empty sample' }
  foreach ($value in $Values) {
    if ([double]::IsNaN($value) -or [double]::IsInfinity($value) -or $value -lt 0) { throw 'Invalid measurement' }
  }
  $sorted = @($Values | Sort-Object)
  $middle = [int][Math]::Floor($sorted.Count / 2)
  $median = if ($sorted.Count % 2) { $sorted[$middle] } else { ($sorted[$middle - 1] + $sorted[$middle]) / 2.0 }
  [ordered]@{
    count = $Values.Count; min = $sorted[0]; median = $median
    p95 = $sorted[[int][Math]::Ceiling($sorted.Count * 0.95) - 1]; max = $sorted[-1]
  }
}

function Get-ImportBenchmarkSummary {
  param([object[]]$Results)
  @($Results | Group-Object condition | ForEach-Object {
    $group = @($_.Group)
    $workflow = @($group | ForEach-Object { ($_.payload.report.samples | Where-Object { $_.phase -eq 'workflow' }).elapsedNs / 1e9 })
    $rss = @($group | ForEach-Object { [double]$_.payload.report.processPeakRssMb })
    $handles = @($group | ForEach-Object { [double]$_.process.sampledPeakHandles })
    $phaseStats = @(foreach ($scope in @('samples', 'partitionAggregates')) {
      $phaseNames = @($group[0].payload.report.$scope | ForEach-Object { $_.phase } | Sort-Object -Unique)
      foreach ($phase in $phaseNames) {
        $values = @(foreach ($trial in $group) {
          $samples = @($trial.payload.report.$scope | Where-Object { $_.phase -eq $phase -and ($scope -eq 'partitionAggregates' -or $null -eq $_.PSObject.Properties['partitionIndex']) })
          if ($samples.Count -eq 1 -and $samples[0].PSObject.Properties['elapsedNs']) { $samples[0].elapsedNs / 1e9 }
        })
        if ($values.Count -ne $group.Count) { continue }
        [ordered]@{ phase = $phase; scope = if ($scope -eq 'samples') { 'source-wall-inclusive' } else { 'partition-work-sum' }
          seconds = Get-ImportBenchmarkStatistics -Values $values
          counters = @(Get-ImportBenchmarkPhaseCounters -Trials $group -Scope $scope -Phase $phase) }
      }
    })
    [ordered]@{
      condition = $_.Name; input = $group[0].input; workersRequested = $group[0].payload.workersRequested
      workersResolved = $group[0].payload.workersResolved
      workflowSeconds = Get-ImportBenchmarkStatistics -Values $workflow
      importProcessPeakRssMb = Get-ImportBenchmarkStatistics -Values $rss
      sampledChildPeakHandles = Get-ImportBenchmarkStatistics -Values $handles
      processMetrics = @(Get-ImportBenchmarkProcessStatistics -Trials $group)
      phases = $phaseStats
    }
  })
}

function Get-ImportBenchmarkPhaseCounters {
  param([object[]]$Trials, [string]$Scope, [string]$Phase)
  foreach ($metric in @('maxBufferedBatches', 'maxBufferedRecords', 'mainTableUpdatedRows', 'materializedPathRows', 'stagingLogicalBytes', 'sampledWalMaxBytes')) {
    $values = @(foreach ($trial in $Trials) {
      $samples = @($trial.payload.report.$Scope | Where-Object { $_.phase -eq $Phase -and
          ($Scope -eq 'partitionAggregates' -or $null -eq $_.PSObject.Properties['partitionIndex']) })
      if ($samples.Count -ne 1) { continue }
      $property = $samples[0].PSObject.Properties[$metric]
      if ($null -ne $property -and $null -ne $property.Value) { [double]$property.Value }
    })
    if ($values.Count -eq 0) { continue }
    [ordered]@{ metric = $metric; observations = $values.Count; expectedObservations = $Trials.Count
      availableForAllTrials = $values.Count -eq $Trials.Count
      partitionAggregation = if ($metric -in @('mainTableUpdatedRows','materializedPathRows')) { 'sum' } else { 'max-single-partition' }
      statistics = Get-ImportBenchmarkStatistics -Values $values }
  }
}

function Get-ImportBenchmarkProcessStatistics {
  param([object[]]$Trials)
  foreach ($metric in @(
      @{ field = 'userCpuNs'; name = 'userCpuSeconds'; divisor = 1e9; unit = 'seconds' },
      @{ field = 'kernelCpuNs'; name = 'kernelCpuSeconds'; divisor = 1e9; unit = 'seconds' },
      @{ field = 'cpuCoreEquivalents'; name = 'cpuCoreEquivalents'; divisor = 1; unit = 'cores' },
      @{ field = 'processReadTransferBytes'; name = 'processReadTransferBytes'; divisor = 1; unit = 'bytes' },
      @{ field = 'processWriteTransferBytes'; name = 'processWriteTransferBytes'; divisor = 1; unit = 'bytes' },
      @{ field = 'processReadOperations'; name = 'processReadOperations'; divisor = 1; unit = 'operations' },
      @{ field = 'processWriteOperations'; name = 'processWriteOperations'; divisor = 1; unit = 'operations' },
      @{ field = 'handlesAtStart'; name = 'handlesAtStart'; divisor = 1; unit = 'handles' },
      @{ field = 'handlesAtEnd'; name = 'handlesAtEnd'; divisor = 1; unit = 'handles' }
    )) {
    $values = @(foreach ($trial in $Trials) {
      if (-not $trial.payload.report.PSObject.Properties['processUsage']) { continue }
      $usage = $trial.payload.report.processUsage
      if ($null -eq $usage -or $usage.scope -cne 'workflowProcessWindow') { continue }
      $property = $usage.PSObject.Properties[$metric.field]
      if ($null -ne $property -and $null -ne $property.Value) { [double]$property.Value / $metric.divisor }
    })
    [ordered]@{
      metric = $metric.name; unit = $metric.unit; scope = 'workflow-process-window'
      observations = $values.Count; expectedObservations = $Trials.Count
      availableForAllTrials = $values.Count -eq $Trials.Count
      statistics = if ($values.Count) { Get-ImportBenchmarkStatistics -Values $values } else { $null }
    }
  }
}

Export-ModuleMember -Function Assert-ImportBenchmarkPayload, Assert-ImportBenchmarkOracle, Get-ImportBenchmarkStatistics, Get-ImportBenchmarkSummary
