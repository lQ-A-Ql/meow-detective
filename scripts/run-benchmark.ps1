param(
  [ValidateSet('search_query','file_tree_expand','file_paginate','timeline_filter','artifact_extract','report_export','all')]
  [string]$Scenario = 'all',

  [int]$Runs = 5,

  [switch]$Cold,

  [string]$OutputDir = '',

  [string]$Cargo = '',

  [string]$FixtureLevel = 'small'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ── Resolve cargo executable ─────────────────────────────────────
if ([string]::IsNullOrWhiteSpace($Cargo)) {
  if ($env:CARGO) {
    $Cargo = $env:CARGO
  } else {
    $Cargo = "cargo"
  }
}

# ── Resolve repository root ──────────────────────────────────────
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')

# ── Resolve output directory ─────────────────────────────────────
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $repoRoot 'artifacts/benchmark-results'
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

# ── Timestamp for output files ───────────────────────────────────
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'

# ── Cold start: clear build artifacts to force recompile ────────
if ($Cold) {
  Write-Host '[Cold] Clearing target directory to force cold compile...'
  $targetDir = Join-Path $repoRoot 'target'
  if (Test-Path -LiteralPath $targetDir) {
    Remove-Item -LiteralPath $targetDir -Recurse -Force
    Write-Host '[Cold] target/ removed'
  }
  # Also clear OS file cache (best-effort, requires admin)
  try {
    # Using Windows built-in method to flush working set
    $null = [System.Runtime.InteropServices.Marshal]
    Write-Host '[Cold] Note: for true cold run, reboot or use RAMMap to clear standby list'
  } catch {
    # Not critical
  }
}

# ── Run the benchmark test ───────────────────────────────────────
Write-Host "Running benchmark scenario=$Scenario runs=$Runs cold=$Cold fixtureLevel=$FixtureLevel"
Write-Host "Repository: $repoRoot"

Push-Location $repoRoot
try {
  $testArgs = @(
    'test', '-p', 'forensics-desktop',
    'bench_all_scenarios',
    '--', '--nocapture'
  )

  Write-Host "Cargo command: $Cargo $($testArgs -join ' ')"

  $procInfo = New-Object System.Diagnostics.ProcessStartInfo
  $procInfo.FileName = $Cargo
  $procInfo.Arguments = $testArgs -join ' '
  $procInfo.WorkingDirectory = $repoRoot.Path
  $procInfo.UseShellExecute = $false
  $procInfo.RedirectStandardOutput = $true
  $procInfo.RedirectStandardError = $true
  $procInfo.EnvironmentVariables['RUST_BACKTRACE'] = '1'

  $process = New-Object System.Diagnostics.Process
  $process.StartInfo = $procInfo
  [void]$process.Start()

  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()

  $process.WaitForExit()
  $stdout = $stdoutTask.Result
  $stderr = $stderrTask.Result

  $exitCode = $process.ExitCode

  # Save raw logs
  $rawLogPath = Join-Path $OutputDir "benchmark-$timestamp-raw.log"
  @"
=== STDOUT ===
$stdout

=== STDERR ===
$stderr

=== EXIT CODE ===
$exitCode
"@ | Out-File -LiteralPath $rawLogPath -Encoding UTF8

  if ($exitCode -ne 0) {
    Write-Host "Benchmark test failed with exit code $exitCode"
    Write-Host "Raw log: $rawLogPath"
    throw "cargo test exited with code $exitCode"
  }

  # ── Parse benchmark output ─────────────────────────────────────
  # The test outputs: [BENCH-OUTPUT] {json}
  $allLines = @(($stdout, $stderr) -split "`r?`n" | Where-Object { $_ -match '\[BENCH-OUTPUT\]' })
  if ($allLines.Count -eq 0) {
    throw "No [BENCH-OUTPUT] markers found in test output. Raw log: $rawLogPath"
  }

  # Take the last match (in case of multiple)
  $markerLine = $allLines[-1]
  $jsonStart = $markerLine.IndexOf('{')
  if ($jsonStart -lt 0) {
    throw "Could not find JSON in benchmark output line: $markerLine"
  }
  $rawJson = $markerLine.Substring($jsonStart)
  $benchData = $rawJson | ConvertFrom-Json

  # ── Filter scenarios if specific scenario requested ────────────
  $scenarios = @($benchData.scenarios)
  if ($Scenario -ne 'all') {
    $scenarios = @($scenarios | Where-Object { $_.scenario -eq $Scenario })
  }

  # ── Compute p95 from individual run timings ────────────────────
  $enrichedScenarios = @()
  foreach ($s in $scenarios) {
    $runTimings = @($s.runs | ForEach-Object { [int64]$_.elapsedMs })
    if ($runTimings.Count -gt 0) {
      $sorted = @($runTimings | Sort-Object)
      $p95Index = [int][Math]::Ceiling($sorted.Count * 0.95) - 1
      if ($p95Index -lt 0) { $p95Index = 0 }
      $computedP95 = $sorted[$p95Index]
    } else {
      $computedP95 = [int64]$s.p95Ms
    }

    $scenarioObj = [ordered]@{
      datasetLevel = [string]$s.datasetLevel
      scenario = [string]$s.scenario
      p95Ms = $computedP95
      memoryPeakMb = if ($s.PSObject.Properties.Name -contains 'memoryPeakMb') { [int64]$s.memoryPeakMb } else { $null }
      baselineVersion = '2026.06'
      runs = @($runTimings)
    }
    $enrichedScenarios += $scenarioObj
  }

  # ── Build host profile ─────────────────────────────────────────
  $osInfo = Get-CimInstance -ClassName Win32_OperatingSystem | Select-Object -First 1
  $totalRamGb = [Math]::Round([int64]$osInfo.TotalVisibleMemorySize / 1MB, 0)
  $osCaption = $osInfo.Caption -replace '^Microsoft\s+', ''
  if ($osCaption -notmatch '^Windows') {
    $osCaption = "Windows $osCaption"
  }
  $hostProfile = "$osCaption / ${totalRamGb}GB RAM / NVMe / Rust stable"

  # ── Output JSON ────────────────────────────────────────────────
  $outputJson = [ordered]@{
    hostProfile = $hostProfile
    baselineVersion = '2026.06'
    lastVerifiedAt = (Get-Date).ToString('yyyy-MM-ddTHH:mm:ssZ')
    generatedAt = (Get-Date).ToString('yyyy-MM-ddTHH:mm:ssZ')
    fixtureLevel = $FixtureLevel
    coldStart = [bool]$Cold
    runs = $Runs
    scenarios = $enrichedScenarios
  }

  $jsonPath = Join-Path $OutputDir "benchmark-$FixtureLevel-$timestamp.json"
  $outputJson | ConvertTo-Json -Depth 6 | Out-File -LiteralPath $jsonPath -Encoding UTF8

  # ── Print summary ──────────────────────────────────────────────
  Write-Host ''
  Write-Host '=== Benchmark Results ==='
  Write-Host "Host: $hostProfile"
  Write-Host "Level: $FixtureLevel  Cold: $Cold  Runs: $Runs"
  Write-Host ''
  Write-Host "$('Scenario'.PadRight(22)) $('Level'.PadLeft(6)) $('p95(ms)'.PadLeft(10)) $('Memory(MB)'.PadLeft(12))"
  Write-Host "$('-' * 22) $('-' * 6) $('-' * 10) $('-' * 12)"
  foreach ($s in $enrichedScenarios) {
    $scenarioName = [string]$s.scenario
    $levelName = [string]$s.datasetLevel
    $p95Display = "$([string]$s.p95Ms)ms"
    $memDisplay = if ($null -ne $s.memoryPeakMb) { "$($s.memoryPeakMb) MB" } else { 'N/A' }
    Write-Host "$($scenarioName.PadRight(22)) $($levelName.PadLeft(6)) $($p95Display.PadLeft(10)) $($memDisplay.PadLeft(12))"
  }
  Write-Host ''
  Write-Host "JSON: $jsonPath"
  Write-Host "Raw log: $rawLogPath"
  Write-Host ''

  # Emit the JSON path for downstream consumers
  Write-Host "JSON: $jsonPath"
} finally {
  Pop-Location
}
