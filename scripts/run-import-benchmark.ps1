<#
.SYNOPSIS
Repeatable NTFS import matrix with per-process measurements and correctness oracles.
.EXAMPLE
./scripts/run-import-benchmark.ps1 -Records 100000 -Runs 5 -WarmupRuns 1 -Release
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
  [switch]$Release
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
foreach ($module in @('Process', 'Input', 'Report')) {
  Import-Module (Join-Path $PSScriptRoot "lib/ImportBenchmark.$module.psm1") -Force
}
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (-not $env:OS -or $env:OS -ne 'Windows_NT') { throw 'This benchmark currently targets Windows' }
if ($Workers.Count -eq 0 -or $Formats.Count -eq 0) { throw 'Select at least one worker budget and format' }
if ($FixturePath -and ($PSBoundParameters.ContainsKey('Records') -or $PSBoundParameters.ContainsKey('Formats'))) {
  throw 'Choose FixturePath or synthetic Records/Formats'
}
if ($FixturePath) { $FixturePath = (Resolve-Path -LiteralPath $FixturePath).Path }
if (-not $OutputDir) { $OutputDir = Join-Path $repo 'artifacts/import-benchmarks' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputDir)) ('matrix-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$null = New-Item -ItemType Directory -Path $directory
Write-Host "Benchmark artifacts: $directory"
$code = Get-ImportBenchmarkCodeContext -RepoRoot $repo
$codeJson = $code | ConvertTo-Json -Depth 6 -Compress
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
$conditions = @(foreach ($inputIndex in 0..($inputs.Count - 1)) {
  foreach ($budget in ($Workers | Sort-Object -Unique)) {
    [pscustomobject]@{ key = "$($inputs[$inputIndex].format)-w$budget"; input = $inputIndex; workers = $budget }
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
$json = Join-Path $directory 'report.json'
[IO.File]::WriteAllText($json, ($report | ConvertTo-Json -Depth 30), (New-Object Text.UTF8Encoding($false)))
Write-Host "Validated matrix report: $json"
