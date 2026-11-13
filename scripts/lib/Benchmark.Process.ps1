function Invoke-BenchmarkProcess {
  param([string]$Repository, [string]$Cargo, [string]$Scenario, [int]$Runs,
    [int]$WarmupRuns, [string]$FixtureLevel, [string]$BuildProfile,
    [string]$Scratch, [string]$LogPath)
  $arguments = @('test', '-p', 'forensics-desktop', '--lib')
  if ($BuildProfile -eq 'release') { $arguments += '--release' }
  $arguments += @('commands::tests::bench_all_scenarios', '--', '--exact', '--ignored', '--nocapture', '--test-threads=1')
  $info = [Diagnostics.ProcessStartInfo]::new()
  $info.FileName = $Cargo
  $info.Arguments = $arguments -join ' '
  $info.WorkingDirectory = $Repository
  $info.UseShellExecute = $false
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.EnvironmentVariables['FORENSICS_BENCH_SCENARIO'] = $Scenario
  $info.EnvironmentVariables['FORENSICS_BENCH_RUNS'] = [string]$Runs
  $info.EnvironmentVariables['FORENSICS_BENCH_WARMUP_RUNS'] = [string]$WarmupRuns
  $info.EnvironmentVariables['FORENSICS_BENCH_FIXTURE_LEVEL'] = $FixtureLevel
  $info.EnvironmentVariables['TEMP'] = $Scratch
  $info.EnvironmentVariables['TMP'] = $Scratch
  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $info
  try {
    [void]$process.Start()
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $stdout = $stdoutTask.Result
    $stderr = $stderrTask.Result
    @("STDOUT:", $stdout, "STDERR:", $stderr, "EXIT CODE: $($process.ExitCode)") |
      Set-Content -LiteralPath $LogPath -Encoding UTF8
    if ($process.ExitCode -ne 0) { throw "Benchmark failed: $Scenario; log: $LogPath" }
    $markers = @((($stdout, $stderr) -split "`r?`n") | Where-Object { $_.StartsWith('[BENCH-OUTPUT] ') })
    if ($markers.Count -ne 1) { throw "Expected one benchmark payload for $Scenario; log: $LogPath" }
    return ($markers[0].Substring('[BENCH-OUTPUT] '.Length) | ConvertFrom-Json)
  } finally { $process.Dispose() }
}
