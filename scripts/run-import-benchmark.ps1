<#
.SYNOPSIS
Repeatable NTFS import matrix with per-process measurements and correctness oracles.
.EXAMPLE
./scripts/run-import-benchmark.ps1 -Records 100000 -Runs 5 -WarmupRuns 1 -Release -RunKind Formal
.EXAMPLE
./scripts/run-import-benchmark.ps1 -FixturePath C:\evidence\disk.E01 -Runs 5 -Release
#>
[CmdletBinding()]
param(
  [string]$FixturePath = '',
  [ValidateRange(32, 1100000)][int]$Records = 100000,
  [ValidateSet('raw', 'e01')][string[]]$Formats = @('raw', 'e01'),
  [ValidateRange(1, 4)][int[]]$Workers = @(1, 2, 4),
  [ValidateRange(1, 100)][int]$Runs = 5,
  [ValidateRange(0, 10)][int]$WarmupRuns = 1,
  [int]$Seed = 20261016,
  [string]$OutputDir = '',
  [ValidateSet('Exploratory', 'Formal', 'Pressure')][string]$RunKind = 'Exploratory',
  [string]$ProtocolPath = '',
  [switch]$Release
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
foreach ($module in @('Process', 'Input', 'Report', 'Protocol')) {
  Import-Module (Join-Path $PSScriptRoot "lib/ImportBenchmark.$module.psm1") -Force
}
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (-not $env:OS -or $env:OS -ne 'Windows_NT') { throw 'This benchmark currently targets Windows' }
if ($Workers.Count -eq 0 -or $Formats.Count -eq 0) { throw 'Select at least one worker budget and format' }
if ($PSBoundParameters.ContainsKey('FixturePath') -and [string]::IsNullOrWhiteSpace($FixturePath)) {
  throw 'An explicit FixturePath must not be empty; a missing real input is not a synthetic trial'
}
if (-not $ProtocolPath) { $ProtocolPath = Join-Path $PSScriptRoot 'baselines/import-optimization-protocol.json' }
$protocol = Read-ImportBenchmarkProtocol -Path $ProtocolPath
Assert-ImportBenchmarkRunConfiguration $protocol $RunKind $Runs $WarmupRuns ([bool]$Release)
if ($FixturePath -and ($PSBoundParameters.ContainsKey('Records') -or $PSBoundParameters.ContainsKey('Formats'))) {
  throw 'Choose FixturePath or synthetic Records/Formats'
}
if ($FixturePath) {
  $FixturePath = (Resolve-Path -LiteralPath $FixturePath).Path
  $null = @(Get-ImportBenchmarkSegments -Path $FixturePath)
}
if (-not $OutputDir) { $OutputDir = Join-Path $repo 'artifacts/import-benchmarks' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputDir)) ('matrix-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$null = New-Item -ItemType Directory -Path $directory
Write-Host "Benchmark artifacts: $directory"
$code = Get-ImportBenchmarkCodeContext -RepoRoot $repo
$codeJson = $code | ConvertTo-Json -Depth 6 -Compress
$configurationPath = Join-Path $directory 'run-configuration.json'
$configuration = [ordered]@{
  schemaVersion = 1; protocolId = $protocol.definition.protocolId; protocolSha256 = $protocol.sha256
  runKind = $RunKind; release = [bool]$Release; workers = @($Workers | Sort-Object -Unique)
  runs = $Runs; warmupRuns = $WarmupRuns; seed = $Seed
  fixturePath = if ($FixturePath) { $FixturePath } else { $null }
  syntheticRecords = if ($FixturePath) { $null } else { $Records }
  formats = if ($FixturePath) { @() } else { @($Formats | Sort-Object -Unique) }
}
[IO.File]::WriteAllText($configurationPath, ($configuration | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
$configurationHash = (Get-FileHash -LiteralPath $configurationPath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $directory 'code-context.json'), ($code | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
$executable = Get-ImportBenchmarkExecutable -RepoRoot $repo -Directory $directory -Release:$Release
$executableHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
$hostInfo = [ordered]@{ os = [Environment]::OSVersion.VersionString; processors = [Environment]::ProcessorCount
  cpuName = $null; physicalMemoryBytes = $null; hardwareDetails = 'available' }
try {
  $hostInfo.cpuName = (Get-CimInstance Win32_Processor -ErrorAction Stop | Select-Object -First 1).Name
  $hostInfo.physicalMemoryBytes = (Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).TotalPhysicalMemory
} catch { $hostInfo.hardwareDetails = 'unavailable' }
$paths = @()
$logicalHash = $null
if ($FixturePath) { $paths = @($FixturePath) } else {
  $fixtureDirectory = Join-Path $directory 'fixtures'
  $log = Join-Path $directory 'prepare.log'
  $null = Invoke-ImportBenchmarkProcess -Executable $executable -WorkingDirectory $repo -Log $log `
    -Arguments @('prepare_import_benchmark_fixture', '--exact', '--ignored', '--nocapture', '--test-threads=1') `
    -Environment @{ FORENSICS_IMPORT_BENCH_DIR = $fixtureDirectory; FORENSICS_IMPORT_BENCH_RECORDS = $Records }
  $prepared = @([IO.File]::ReadLines($log) | Where-Object { $_ -match '^\[fixture-prepared\] records=\d+ logicalSha256=[0-9a-f]{64}$' })
  if ($prepared.Count -ne 1) { throw "Missing fixture preparation result; see $log" }
  $logicalHash = $prepared[0].Split('=')[-1]
  $paths = @($Formats | Sort-Object -Unique | ForEach-Object { Join-Path $fixtureDirectory "synthetic.$_" })
}
$inputs = @($paths | ForEach-Object { Get-ImportBenchmarkManifest -Path $_ })
[IO.File]::WriteAllText((Join-Path $directory 'inputs.json'), (ConvertTo-Json -InputObject $inputs -Depth 10), (New-Object Text.UTF8Encoding($false)))
$conditions = @(foreach ($inputIndex in 0..($inputs.Count - 1)) {
  foreach ($budget in ($Workers | Sort-Object -Unique)) {
    [pscustomobject]@{ key = "$($inputs[$inputIndex].format)-w$budget"; input = $inputIndex; workers = $budget
      oracleGroup = if ($FixturePath) { [string]$inputIndex } else { 'synthetic-logical-image' } }
  }
})
$results = New-Object 'System.Collections.Generic.List[object]'
$warmups = New-Object 'System.Collections.Generic.List[object]'
$oracles = @{}
$random = New-Object Random($Seed)
$sequence = 0
for ($round = 1; $round -le ($WarmupRuns + $Runs); $round++) {
  $order = @($conditions)
  # Seeded Fisher-Yates keeps exactly one trial per condition per round.
  for ($index = $order.Count - 1; $index -gt 0; $index--) {
    $swap = $random.Next($index + 1)
    $temporary = $order[$index]; $order[$index] = $order[$swap]; $order[$swap] = $temporary
  }
  foreach ($condition in $order) {
    $sequence++
    $log = Join-Path $directory ('{0:000}-{1}.log' -f $sequence, $condition.key)
    $process = Invoke-ImportBenchmarkProcess -Executable $executable -WorkingDirectory $repo -Log $log `
      -Arguments @('matrix_import_profile', '--exact', '--ignored', '--nocapture', '--test-threads=1') `
      -Environment @{ FORENSICS_IMPORT_FIXTURE = $inputs[$condition.input].path; FORENSICS_IMPORT_BENCH_WORKERS = $condition.workers }
    $payload = Read-ImportBenchmarkPayload -Log $log
    Assert-ImportBenchmarkPayload -Payload $payload -Workers $condition.workers
    # Synthetic RAW and E01 represent identical logical bytes; real input is
    # compared only with itself, across workers, warmups, and repetitions.
    $oracleKey = if ($FixturePath) { [string]$condition.input } else { 'synthetic-logical-image' }
    if ($oracles.ContainsKey($oracleKey)) { Assert-ImportBenchmarkOracle -Expected $oracles[$oracleKey] -Actual $payload }
    else { $oracles[$oracleKey] = $payload }
    $trial = [ordered]@{ sequence = $sequence; round = $round; condition = $condition.key; input = $condition.input
      warmup = $round -le $WarmupRuns; process = $process; payload = $payload }
    if ($trial.warmup) { $warmups.Add([pscustomobject]$trial) } else { $results.Add([pscustomobject]$trial) }
    Write-Host "$sequence/$($conditions.Count * ($WarmupRuns + $Runs)): $($condition.key) warmup=$($trial.warmup) rows=$($payload.catalogRows)"
  }
}
foreach ($inputManifest in $inputs) {
  $after = Get-ImportBenchmarkManifest -Path $inputManifest.path
  if ($after.manifestSha256 -cne $inputManifest.manifestSha256) { throw 'Input changed during matrix; reject all timings' }
}
if ($codeJson -cne ((Get-ImportBenchmarkCodeContext -RepoRoot $repo) | ConvertTo-Json -Depth 6 -Compress) -or
    (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $executableHash) {
  throw 'Code or test executable changed during matrix; reject mixed-version results'
}
$report = [ordered]@{
  schemaVersion = 2; completedUtc = [DateTime]::UtcNow.ToString('o'); codeContext = $code
  protocol = $protocol; runKind = $RunKind; configuration = $configuration; configurationSha256 = $configurationHash
  executableSha256 = $executableHash; rustc = @(& rustc -Vv); cargo = @(& cargo -V)
  buildEnvironment = [ordered]@{ rustflags = $env:RUSTFLAGS; cargoBuildTarget = $env:CARGO_BUILD_TARGET
    cargoIncremental = $env:CARGO_INCREMENTAL; rayonNumThreads = $env:RAYON_NUM_THREADS }
  buildProfile = if ($Release) { 'release' } else { 'test' }; hostInfo = $hostInfo
  cacheCondition = if ($WarmupRuns -gt 0) { 'warm-by-full-import' } else { 'uncontrolled' }
  cacheNotes = 'Input SHA256 reads precede trials; OS caches are never cleared. Each trial uses a fresh process and case. Timings exclude oracle and case cleanup.'
  syntheticRecords = if ($FixturePath) { $null } else { $Records }
  syntheticLayout = if ($FixturePath) { $null } else { 'resident-files-empty-root-index' }
  syntheticLogicalSha256 = $logicalHash; inputs = $inputs; seed = $Seed; runs = $Runs; warmupRuns = $WarmupRuns
  statistics = 'median and nearest-rank p95; partition phase durations are work sums, never workflow wall time'
  summary = @(Get-ImportBenchmarkSummary -Results $results.ToArray()); warmups = $warmups.ToArray(); results = $results.ToArray()
}
$afterProtocol = Read-ImportBenchmarkProtocol -Path $protocol.path
if ($afterProtocol.sha256 -cne $protocol.sha256 -or
    (Get-FileHash -LiteralPath $configurationPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $configurationHash) {
  throw 'Protocol or run configuration changed during measurement; reject all timings'
}
$report.assessment = Get-ImportBenchmarkRunAssessment -Protocol $protocol -RunKind $RunKind -Conditions $conditions `
  -Results $results.ToArray() -Warmups $warmups.ToArray() -Runs $Runs -WarmupRuns $WarmupRuns -Release ([bool]$Release)
$json = Join-Path $directory 'report.json'
[IO.File]::WriteAllText($json, ($report | ConvertTo-Json -Depth 30), (New-Object Text.UTF8Encoding($false)))
Write-Host "Validated matrix report: $json"
if (-not $report.assessment.measurementGatesPassed) { throw "Benchmark gates failed; see $json" }
