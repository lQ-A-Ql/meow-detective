param(
  [string]$FixturePath = $env:FORENSICS_IMPORT_FIXTURE,
  [int]$SyntheticRecords = 0,
  [int]$Runs = 3,
  [string]$OutputDir = '',
  [switch]$Release
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($Runs -lt 1) { throw 'Runs must be positive' }
if ($SyntheticRecords -gt 0 -and -not [string]::IsNullOrWhiteSpace($FixturePath)) {
  throw 'Choose either FixturePath or SyntheticRecords'
}
if ($SyntheticRecords -eq 0 -and [string]::IsNullOrWhiteSpace($FixturePath)) {
  throw 'Pass FixturePath / FORENSICS_IMPORT_FIXTURE, or SyntheticRecords'
}
if ($SyntheticRecords -ne 0 -and ($SyntheticRecords -lt 32 -or $SyntheticRecords -gt 1100000)) {
  throw 'SyntheticRecords must be between 32 and 1100000'
}
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDir)) { $OutputDir = Join-Path $repoRoot 'artifacts/import-profiles' }
$null = New-Item -ItemType Directory -Path $OutputDir -Force
$runName = 'import-profile-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
$oldFixture = $env:FORENSICS_IMPORT_FIXTURE
$oldCount = $env:FORENSICS_IMPORT_BENCH_RECORDS
$results = @()
$fixtureHash = $null

function Get-CodeContext {
  $changes = @(& git -C $repoRoot diff --name-only HEAD) + @(& git -C $repoRoot ls-files --others --exclude-standard)
  $changes = @($changes | Where-Object { $_ -match '(\.rs$|Cargo\.(toml|lock)$|rust-toolchain\.toml$|^scripts/run-import-profile\.ps1$)' } | Sort-Object -Unique)
  $fingerprints = @($changes | ForEach-Object {
    $path = Join-Path $repoRoot $_
    [ordered]@{ path = $_; sha256 = if (Test-Path -LiteralPath $path -PathType Leaf) { (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null } }
  })
  [ordered]@{ revision = (& git -C $repoRoot rev-parse HEAD).Trim(); changedCode = $fingerprints }
}

$codeContext = Get-CodeContext
$hostInfo = [ordered]@{ os = [Environment]::OSVersion.VersionString; processorCount = [Environment]::ProcessorCount; cpuName = $null; physicalMemoryBytes = $null; hardwareDetails = 'available' }
try {
  $hostInfo.cpuName = (Get-CimInstance Win32_Processor -ErrorAction Stop | Select-Object -First 1).Name
  $hostInfo.physicalMemoryBytes = (Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).TotalPhysicalMemory
} catch { $hostInfo.hardwareDetails = 'unavailable' }
try {
  if ($SyntheticRecords -gt 0) {
    $testName = 'synthetic_import_profile'
    $env:FORENSICS_IMPORT_BENCH_RECORDS = [string]$SyntheticRecords
  } else {
    $fixture = (Resolve-Path -LiteralPath $FixturePath).Path
    $testName = 'fixture_import_profile'
    $env:FORENSICS_IMPORT_FIXTURE = $fixture
    # This identifies the selected input file, not the full E01 container set.
    $fixtureHash = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash.ToLowerInvariant()
  }
  $cargoArgs = @('test', '-p', 'app-services', '--test', 'import_benchmark')
  if ($Release) { $cargoArgs += '--release' }
  $cargoArgs += @($testName, '--', '--ignored', '--exact', '--nocapture', '--test-threads=1')
  for ($run = 1; $run -le $Runs; $run++) {
    $log = Join-Path $OutputDir "$runName-run-$run.log"
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = (Get-Command cargo -ErrorAction Stop).Source
    # All arguments are fixed test names/options or switches, never input paths.
    $startInfo.Arguments = [string]::Join(' ', $cargoArgs)
    $startInfo.WorkingDirectory = $repoRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $profileProcess = New-Object System.Diagnostics.Process
    $profileProcess.StartInfo = $startInfo
    try {
      [void]$profileProcess.Start()
      $stdoutTask = $profileProcess.StandardOutput.ReadToEndAsync()
      $stderrTask = $profileProcess.StandardError.ReadToEndAsync()
      $profileProcess.WaitForExit()
      $stdout = $stdoutTask.GetAwaiter().GetResult()
      $stderr = $stderrTask.GetAwaiter().GetResult()
      ($stdout + [Environment]::NewLine + $stderr) | Set-Content -LiteralPath $log -Encoding UTF8
      $exitCode = $profileProcess.ExitCode
    } finally { $profileProcess.Dispose() }
    if ($exitCode -ne 0) { throw "Import benchmark run $run failed. See $log" }
    $machineLines = @(Get-Content -LiteralPath $log | Where-Object { $_ -match '^\[import-benchmark\] ' })
    if ($machineLines.Count -ne 1) { throw "Expected exactly one benchmark report in $log" }
    $result = $machineLines[0].Substring('[import-benchmark] '.Length) | ConvertFrom-Json
    if ($result.schemaVersion -ne 1 -or $result.report.schemaVersion -ne 1) { throw 'Unsupported report version' }
    $workflow = @($result.report.samples | Where-Object { $_.phase -eq 'workflow' })
    if ($workflow.Count -ne 1 -or $workflow[0].state -ne 'completed' -or $null -eq $workflow[0].elapsedNs) {
      throw "Missing complete workflow duration in $log"
    }
    $read = @($result.report.partitionAggregates | Where-Object { $_.phase -eq 'mftRead' })
    if ($read.Count -ne 1 -or $read[0].scannedRecords -le 0) { throw "No NTFS MFT work in $log" }
    $result | Add-Member -NotePropertyName run -NotePropertyValue $run
    $result | Add-Member -NotePropertyName log -NotePropertyValue $log
    $results += $result
    Write-Host "Run $run/$Runs completed: $($result.catalogRows) catalog rows"
  }
} finally {
  $env:FORENSICS_IMPORT_FIXTURE = $oldFixture
  $env:FORENSICS_IMPORT_BENCH_RECORDS = $oldCount
}
if (($codeContext | ConvertTo-Json -Depth 8 -Compress) -ne ((Get-CodeContext) | ConvertTo-Json -Depth 8 -Compress)) {
  throw 'Code changed while profiling; do not compare a mixed-version run'
}
foreach ($result in $results) {
  foreach ($key in @('catalogSha256', 'graphSha256', 'timelineSha256')) {
    if ($result.$key -ne $results[0].$key) { throw "Correctness oracle changed across runs: $key" }
  }
}
$times = @($results | ForEach-Object {
  ($_.report.samples | Where-Object { $_.phase -eq 'workflow' }).elapsedNs / 1000000000.0
} | Sort-Object)
$middle = [int][Math]::Floor($times.Count / 2)
$median = if ($times.Count % 2) { $times[$middle] } else { ($times[$middle - 1] + $times[$middle]) / 2.0 }
$summary = [ordered]@{
  schemaVersion = 1
  buildProfile = if ($Release) { 'release' } else { 'test' }
  cacheCondition = 'uncontrolled'
  gitRevision = $codeContext.revision
  codeContext = $codeContext
  hostInfo = $hostInfo
  syntheticRecords = $SyntheticRecords
  syntheticLayout = if ($SyntheticRecords -gt 0) { 'resident-files-empty-root-index' } else { $null }
  selectedInputFileSha256 = $fixtureHash
  totalMedianSeconds = $median
  runs = $Runs
  results = $results
}
$json = Join-Path $OutputDir "$runName.json"
$summary | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $json -Encoding UTF8
Write-Host "JSON: $json"
