<# Frozen executable matrix. Definition JSON names explicit inputs and versions;
   each version declares its supported analysisModes. No builds or automatic retries. #>
[CmdletBinding()]
param([Parameter(Mandatory)][string]$DefinitionPath,
  [Parameter(Mandatory)][string]$OutputDir,
  [ValidateRange(1,100)][int]$Runs = 5, [ValidateRange(0,10)][int]$WarmupRuns = 1,
  [ValidateSet('Exploratory','Formal','Pressure')][string]$RunKind = 'Formal',
  [ValidateRange(1,4)][int[]]$Workers = @(1,2,4), [int]$Seed = 20261016,
  [string]$TempRoot = '', [ValidateRange(1,7200)][int]$TimeoutSeconds = 1800)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
foreach ($name in @('Input','Process','Report','Protocol')) { Import-Module "$PSScriptRoot/lib/ImportBenchmark.$name.psm1" -Force }
$repo = (Resolve-Path "$PSScriptRoot/..").Path
if (-not $TempRoot) { $TempRoot=Join-Path ([IO.Path]::GetPathRoot($repo)) 'ibt' }
$protocol = Read-ImportBenchmarkProtocol "$PSScriptRoot/baselines/import-optimization-protocol.json"
Assert-ImportBenchmarkRunConfiguration $protocol $RunKind $Runs $WarmupRuns $true
$definition = Get-Content -LiteralPath $DefinitionPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($definition.schemaVersion -ne 1 -or -not $definition.versions.Count -or -not $definition.inputs.Count) { throw 'Invalid matrix definition' }
$definitionHash = (Get-FileHash -LiteralPath $DefinitionPath).Hash
foreach ($v in $definition.versions) {
  if ($v.name -notmatch '^[a-zA-Z0-9_-]+$' -or $v.sha256 -notmatch '^[0-9a-fA-F]{64}$' -or
      $v.sha256 -ine (Get-FileHash -LiteralPath $v.executable).Hash -or -not $v.revision) { throw 'Invalid frozen version identity' }
  foreach ($mode in $v.analysisModes) { if ($mode -notin @('metadataOnly','fullContent')) { throw 'Unsupported analysis mode' } }
}
$directory = Join-Path ([IO.Path]::GetFullPath($OutputDir)) ('comparison-' + (Get-Date -Format yyyyMMdd-HHmmss) + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
$null = New-Item -ItemType Directory -Path $directory
$null = New-Item -ItemType Directory -Force -Path $TempRoot
$code = Get-ImportBenchmarkCodeContext $repo
$codeJson = $code | ConvertTo-Json -Depth 8 -Compress
if (@(Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue).Count) { throw 'Finish compilation before starting a frozen matrix' }
$inputs = @($definition.inputs | ForEach-Object { Get-ImportBenchmarkManifest $_.path })
$conditions = @(foreach ($i in 0..($inputs.Count-1)) { foreach ($v in $definition.versions) { foreach ($mode in $v.analysisModes) { foreach ($w in ($Workers | Sort-Object -Unique)) {
  [pscustomobject]@{ key = "$($definition.inputs[$i].name)-$($v.name)-$mode-w$w"; input = $i; version = $v.name; workers = $w; mode = $mode
    oracleGroup = "$($definition.inputs[$i].logicalGroup)-$mode" }
} } } })
if (@($conditions.key | Sort-Object -Unique).Count -ne $conditions.Count) { throw 'Duplicate matrix condition' }
$report = [ordered]@{ schemaVersion = 1; status = 'running'; definition = $definition; definitionSha256 = $definitionHash
  codeContext = $code; protocol = $protocol; inputs = $inputs; conditions = $conditions; runs = $Runs; warmupRuns = $WarmupRuns; seed = $Seed
  rustc = @(& rustc -Vv); cargo = @(& cargo -V); runKind = $RunKind; buildProfile = 'release'
  environment = @{ tempRoot = $TempRoot; cache = 'input hashes read before trials; full import warmup; no OS cache flush'; processIsolation = 'one fresh process/case per trial'
    background = @('cargo','rustc','forensics-desktop' | ForEach-Object { @(Get-Process -Name $_ -ErrorAction SilentlyContinue | Select-Object Id,ProcessName) })
    retryPolicy = 'fail-fast; retain logs, process status and failed case; rerun entire matrix with new id, never replace failed trial'
    rustflags = $env:RUSTFLAGS; rayon = $env:RAYON_NUM_THREADS; cpu = [Environment]::ProcessorCount }
  trials = @(); summary = @(); comparisons = @(); assessment = $null; failure = $null }
function Save-Report { [IO.File]::WriteAllText((Join-Path $directory 'report.json'), ($report | ConvertTo-Json -Depth 40), (New-Object Text.UTF8Encoding($false))) }
Save-Report
$trials = New-Object 'System.Collections.Generic.List[object]'
$oracles = @{}; $sidecars = @{}; $random = New-Object Random($Seed); $sequence = 0
try {
  for ($round=1; $round -le ($Runs+$WarmupRuns); $round++) {
    $order = @($conditions)
    for ($i=$order.Count-1; $i -gt 0; $i--) { $j=$random.Next($i+1); $t=$order[$i]; $order[$i]=$order[$j]; $order[$j]=$t }
    foreach ($c in $order) {
      $sequence++; $v = @($definition.versions | Where-Object name -eq $c.version)[0]
      $scratch = Join-Path $TempRoot ([guid]::NewGuid().ToString('N').Substring(0,8)); $null = New-Item -ItemType Directory -Path $scratch
      $log = Join-Path $directory ('{0:000}-{1}.log' -f $sequence,$c.key)
      $trial = [ordered]@{ sequence=$sequence; round=$round; condition=$c.key; input=$c.input; version=$c.version; mode=$c.mode; warmup=$round -le $WarmupRuns
        status='running'; log=$log; temp=$scratch; process=$null; payload=$null }
      $trials.Add([pscustomobject]$trial); $current = $trials[$trials.Count-1]; $report.trials=$trials.ToArray(); Save-Report
      $current.process = Invoke-ImportBenchmarkProcess -Executable $v.executable -WorkingDirectory $repo -Log $log -TimeoutSeconds $TimeoutSeconds `
        -Arguments @('matrix_import_profile','--exact','--ignored','--nocapture','--test-threads=1') `
        -Environment @{ TEMP=$scratch; TMP=$scratch; FORENSICS_IMPORT_FIXTURE=$inputs[$c.input].path; FORENSICS_IMPORT_BENCH_WORKERS=$c.workers; FORENSICS_IMPORT_BENCH_ANALYSIS=$c.mode }
      $payload = Read-ImportBenchmarkPayload $log
      Assert-ImportBenchmarkPayload $payload $c.workers
      $actualMode = if ($payload.PSObject.Properties['executionMode']) { $payload.executionMode } else { 'metadataOnly' }
      if ($actualMode -cne $c.mode) { throw 'Executable ignored requested analysis mode' }
      if ($payload.report.processPeakRssMb -gt $protocol.definition.gates.maxImportProcessPeakRssMiB) { throw 'Import RSS gate failed' }
      if ($oracles.ContainsKey($c.oracleGroup)) { Assert-ImportBenchmarkOracle $oracles[$c.oracleGroup] $payload } else { $oracles[$c.oracleGroup]=$payload }
      if ($payload.PSObject.Properties['semanticOracle']) {
        $sidecar = $payload.semanticOracle | ConvertTo-Json -Depth 20 -Compress
        if ($sidecars.ContainsKey($c.oracleGroup) -and $sidecars[$c.oracleGroup] -cne $sidecar) { throw 'Semantic oracle differs within logical image/mode' }
        $sidecars[$c.oracleGroup]=$sidecar
      }
      $current.payload=$payload; $current.status='completed'; $report.trials=$trials.ToArray(); Save-Report
      Write-Host "$sequence/$($conditions.Count*($Runs+$WarmupRuns)) $($c.key) warmup=$($current.warmup)"
    }
  }
  foreach ($i in 0..($inputs.Count-1)) { if ((Get-ImportBenchmarkManifest $inputs[$i].path).manifestSha256 -cne $inputs[$i].manifestSha256) { throw 'Input changed during matrix' } }
  foreach ($v in $definition.versions) { if ((Get-FileHash -LiteralPath $v.executable).Hash -ine $v.sha256) { throw 'Executable changed during matrix' } }
  if ($codeJson -cne ((Get-ImportBenchmarkCodeContext $repo) | ConvertTo-Json -Depth 8 -Compress) -or
      (Get-FileHash -LiteralPath $DefinitionPath).Hash -cne $definitionHash -or
      (Read-ImportBenchmarkProtocol $protocol.path).sha256 -cne $protocol.sha256) { throw 'Frozen matrix configuration changed' }
  $report.summary = @(Get-ImportBenchmarkSummary @($trials | Where-Object { -not $_.warmup }))
  $report.assessment = Get-ImportBenchmarkRunAssessment -Protocol $protocol -RunKind $RunKind -Conditions $conditions `
    -Results @($trials | Where-Object { -not $_.warmup }) -Warmups @($trials | Where-Object warmup) -Runs $Runs -WarmupRuns $WarmupRuns -Release $true
  if (-not $report.assessment.measurementGatesPassed) { throw ('Measurement gates failed: '+($report.assessment.violations -join ', ')) }
  $reference = $definition.versions[0].name
  $report.comparisons = @(foreach ($inputIndex in 0..($inputs.Count-1)) { foreach ($candidate in @($definition.versions | Select-Object -Skip 1)) { foreach ($mode in $candidate.analysisModes) { foreach ($worker in ($Workers | Sort-Object -Unique)) {
    $name=$definition.inputs[$inputIndex].name
    $baseline=@($report.summary | Where-Object condition -eq "$name-$reference-$mode-w$worker")
    $next=@($report.summary | Where-Object condition -eq "$name-$($candidate.name)-$mode-w$worker")
    if ($baseline.Count -ne 1 -or $next.Count -ne 1) { continue }
    $medianDelta=100*($next[0].workflowSeconds.median/$baseline[0].workflowSeconds.median-1)
    $p95Delta=100*($next[0].workflowSeconds.p95/$baseline[0].workflowSeconds.p95-1)
    [ordered]@{input=$name; mode=$mode; workers=$worker; baseline=$reference; candidate=$candidate.name
      workflowMedianDeltaPercent=$medianDelta; workflowP95DeltaPercent=$p95Delta
      confirmationRequired=($medianDelta -gt $protocol.definition.gates.maximumOtherWorkflowRegressionPercent -or $p95Delta -gt $protocol.definition.gates.maximumP95RegressionPercent)
      verdict=if ($RunKind -eq 'Formal') {'measured-comparison; optimization-benefit-not-claimed-by-S1'} else {'exploratory-not-acceptance'} }
  } } } })
  $report.status='completed'
} catch {
  if ($trials.Count -and $trials[-1].status -eq 'running') { $trials[-1].status='failed' }
  $report.status='failed'; $report.failure=$_.Exception.Message; $report.trials=$trials.ToArray(); Save-Report
  throw
}
Save-Report
Write-Host "Frozen matrix: $directory/report.json"
