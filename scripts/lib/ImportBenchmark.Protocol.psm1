Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'ImportBenchmark.Report.psm1') -Scope Local

function Read-ImportBenchmarkProtocol {
  param([Parameter(Mandatory = $true)][string]$Path)
  $resolved = (Resolve-Path -LiteralPath $Path -ErrorAction Stop).Path
  $bytes = [IO.File]::ReadAllBytes($resolved)
  $utf8 = New-Object Text.UTF8Encoding($false, $true)
  $definition = $utf8.GetString($bytes).TrimStart([char]0xfeff) | ConvertFrom-Json
  if ($definition.schemaVersion -ne 1 -or $definition.protocolId -notmatch '^[a-z0-9-]+$' -or
      $definition.baselineRevision -notmatch '^[0-9a-f]{40}$') { throw 'Unsupported import optimization protocol' }
  foreach ($value in @($definition.measurement.minimumFormalRuns, $definition.measurement.minimumWarmupRuns,
      $definition.measurement.pressureRuns, $definition.gates.maxImportProcessPeakRssMiB, $definition.gates.confirmationRuns)) {
    if ($value -isnot [ValueType] -or $value -is [bool] -or [double]::IsNaN([double]$value) -or [double]::IsInfinity([double]$value) -or
        $value -le 0 -or $value -ne [Math]::Floor([double]$value)) { throw 'Protocol budgets must be positive integers' }
  }
  if ($definition.measurement.minimumFormalRuns -lt 5 -or $definition.measurement.minimumWarmupRuns -lt 1 -or
      $definition.measurement.pressureRuns -ne 1 -or $definition.measurement.formalRequiresRelease -ne $true -or
      $definition.gates.maxImportProcessPeakRssMiB -gt 2048 -or $definition.gates.confirmationRuns -lt 5) {
    throw 'Protocol weakens the frozen measurement or 2 GiB resource requirements'
  }
  foreach ($value in @($definition.gates.minimumHotspotReductionPercent, $definition.gates.minimumWorkflowReductionPercent,
      $definition.gates.maximumOtherWorkflowRegressionPercent, $definition.gates.maximumP95RegressionPercent)) {
    if ($value -isnot [ValueType] -or $value -is [bool] -or [double]::IsNaN([double]$value) -or [double]::IsInfinity([double]$value) -or
        $value -le 0 -or $value -gt 100) { throw 'Protocol percentage must be finite and in (0,100]' }
  }
  if ($definition.gates.minimumHotspotReductionPercent -lt 10 -or $definition.gates.minimumWorkflowReductionPercent -lt 3 -or
      $definition.gates.maximumOtherWorkflowRegressionPercent -gt 3 -or $definition.gates.maximumP95RegressionPercent -gt 5) {
    throw 'Protocol weakens the frozen performance decision requirements'
  }
  if ($definition.oracle.payloadSchemaVersion -ne 2 -or $definition.oracle.oracleVersion -ne 2 -or
      $definition.oracle.reportSchemaVersion -ne 1 -or
      $definition.oracle.order -cne 'canonical-primary-id; graph-nodes-before-edges' -or
      $definition.measurement.analysisMode -cne 'metadataOnly' -or $definition.measurement.platform -cne 'windows') {
    throw 'Protocol does not match the supported benchmark oracle contract'
  }
  $sha = [Security.Cryptography.SHA256]::Create()
  try { $digest = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
  [pscustomobject]@{ path = $resolved; sha256 = $digest; definition = $definition }
}

function Assert-ImportBenchmarkRunConfiguration {
  param($Protocol, [ValidateSet('Exploratory', 'Formal', 'Pressure')][string]$RunKind,
    [int]$Runs, [int]$WarmupRuns, [bool]$Release)
  if ($Runs -lt 1 -or $WarmupRuns -lt 0) { throw 'Invalid trial count' }
  $measurement = $Protocol.definition.measurement
  if ($RunKind -eq 'Formal' -and ($Runs -lt $measurement.minimumFormalRuns -or
      $WarmupRuns -lt $measurement.minimumWarmupRuns -or -not $Release)) {
    throw 'Formal measurement requires Release, at least five measured trials and one warmup per condition'
  }
  if ($RunKind -eq 'Pressure' -and ($Runs -ne $measurement.pressureRuns -or -not $Release)) {
    throw 'Pressure measurement requires one measured Release trial; it cannot establish a stable speedup'
  }
}

function Get-ImportBenchmarkRunAssessment {
  param($Protocol, [ValidateSet('Exploratory', 'Formal', 'Pressure')][string]$RunKind,
    [object[]]$Conditions, [object[]]$Results, [object[]]$Warmups, [int]$Runs, [int]$WarmupRuns, [bool]$Release)
  Assert-ImportBenchmarkRunConfiguration $Protocol $RunKind $Runs $WarmupRuns $Release
  if ($Conditions.Count -eq 0) { throw 'At least one condition is required' }
  $violations = New-Object 'System.Collections.Generic.List[string]'
  $expected = @{}
  foreach ($condition in $Conditions) {
    if ($expected.ContainsKey($condition.key)) { throw 'Duplicate planned condition' }
    $expected[$condition.key] = $condition
    if (@($Results | Where-Object { $_.condition -ceq $condition.key }).Count -ne $Runs) {
      $violations.Add("Incomplete measured trials: $($condition.key)")
    }
    if (@($Warmups | Where-Object { $_.condition -ceq $condition.key }).Count -ne $WarmupRuns) {
      $violations.Add("Incomplete warmup trials: $($condition.key)")
    }
  }
  $limit = $Protocol.definition.gates.maxImportProcessPeakRssMiB
  $sequences = @{}
  $rounds = @{}
  $oracles = @{}
  foreach ($group in @(@{ rows = $Results; warmup = $false }, @{ rows = $Warmups; warmup = $true })) {
    foreach ($trial in $group.rows) {
      if (-not $expected.ContainsKey($trial.condition)) { $violations.Add('Unplanned condition'); continue }
      $condition = $expected[$trial.condition]
      if ($trial.warmup -ne $group.warmup -or $trial.input -ne $condition.input) { $violations.Add('Trial scope mismatch') }
      $firstRound = if ($group.warmup) { 1 } else { $WarmupRuns + 1 }
      $lastRound = if ($group.warmup) { $WarmupRuns } else { $WarmupRuns + $Runs }
      if ($trial.round -isnot [ValueType] -or $trial.round -is [bool] -or
          $trial.round -lt $firstRound -or $trial.round -gt $lastRound -or $trial.round -ne [Math]::Floor([double]$trial.round)) {
        $violations.Add('Trial round outside the declared measurement scope')
      }
      $roundKey = "$($trial.condition):$($trial.round)"
      if ($rounds.ContainsKey($roundKey)) { $violations.Add('Duplicate trial round') }
      $rounds[$roundKey] = $true
      if ($trial.sequence -isnot [ValueType] -or $trial.sequence -is [bool] -or $trial.sequence -lt 1 -or
          $trial.sequence -gt ($Conditions.Count * ($Runs + $WarmupRuns)) -or
          $trial.sequence -ne [Math]::Floor([double]$trial.sequence)) { $violations.Add('Invalid trial sequence') }
      if ($sequences.ContainsKey([string]$trial.sequence)) { $violations.Add('Duplicate trial sequence') }
      $sequences[[string]$trial.sequence] = $true
      $oracleKey = if ($condition.PSObject.Properties['oracleGroup']) { $condition.oracleGroup } else { [string]$condition.input }
      try {
        Assert-ImportBenchmarkPayload -Payload $trial.payload -Workers $condition.workers
        if ($oracles.ContainsKey($oracleKey)) { Assert-ImportBenchmarkOracle -Expected $oracles[$oracleKey] -Actual $trial.payload }
        else { $oracles[$oracleKey] = $trial.payload }
      }
      catch { $violations.Add("Invalid payload: $($_.Exception.Message)") }
      foreach ($sample in $trial.payload.report.partitionAggregates) {
        if ($sample.PSObject.Properties['state'] -and $sample.state -in @('failed', 'cancelled')) {
          $violations.Add('Failed or cancelled partition work')
        }
      }
      $peak = $trial.payload.report.processPeakRssMb
      if ($null -eq $peak -or $peak -isnot [ValueType] -or $peak -is [bool] -or [double]::IsNaN([double]$peak) -or
          [double]::IsInfinity([double]$peak) -or $peak -le 0 -or $peak -gt $limit) {
        $violations.Add("Invalid or over-budget import RSS: $($trial.condition)")
      }
    }
  }
  [pscustomobject]@{
    protocolId = $Protocol.definition.protocolId; runKind = $RunKind
    measurementGatesPassed = $violations.Count -eq 0
    formalMeasurement = $RunKind -eq 'Formal' -and $violations.Count -eq 0
    coverage = 'selected-inputs-only'; performanceVerdict = 'not-evaluated-without-baseline'
    maxImportProcessPeakRssMiB = $limit; violations = $violations.ToArray()
  }
}

Export-ModuleMember -Function Read-ImportBenchmarkProtocol, Assert-ImportBenchmarkRunConfiguration, Get-ImportBenchmarkRunAssessment
