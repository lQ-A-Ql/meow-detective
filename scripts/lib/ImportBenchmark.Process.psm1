Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Arguments are fixed Cargo/test switches. Evidence paths travel only through
# the child environment, so shell parsing never touches an evidence filename.
function Invoke-ImportBenchmarkProcess {
  param([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory,
    [hashtable]$Environment = @{}, [string]$Log, [int]$TimeoutSeconds = 7200,
    [string]$ObservedDirectory = '')
  if (@($Arguments | Where-Object { $_ -notmatch '^[a-zA-Z0-9_=,:.\-]+$' }).Count) {
    throw 'Process arguments must be fixed switches or identifiers'
  }
  $info = New-Object System.Diagnostics.ProcessStartInfo
  $info.FileName = $Executable
  $info.Arguments = [string]::Join(' ', $Arguments)
  $info.WorkingDirectory = $WorkingDirectory
  $info.UseShellExecute = $false
  $info.CreateNoWindow = $true
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  foreach ($key in $Environment.Keys) {
    if ($null -eq $Environment[$key]) { $info.EnvironmentVariables.Remove($key) }
    else { $info.EnvironmentVariables[$key] = [string]$Environment[$key] }
  }
  $process = New-Object System.Diagnostics.Process
  $process.StartInfo = $info
  $watch = [Diagnostics.Stopwatch]::StartNew()
  $handles = 0
  $observations = 0
  $peakBytes = 0L
  $visibleBytes = 0L
  $walBytes = 0L
  $exitCode = -1
  $timedOut = $false
  try {
    [void]$process.Start()
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    do {
      try {
        $process.Refresh()
        $handles = [Math]::Max($handles, $process.HandleCount)
        $peakBytes = [Math]::Max($peakBytes, $process.PeakWorkingSet64)
        $observations++
        if ($ObservedDirectory) {
          $total = 0L; $wal = 0L
          try {
            foreach ($file in [IO.Directory]::EnumerateFiles($ObservedDirectory, '*', [IO.SearchOption]::AllDirectories)) {
              try {
                $size = (New-Object IO.FileInfo($file)).Length
                $total += $size
                if ($file.EndsWith('-wal')) { $wal += $size }
              } catch [IO.IOException] { } # Child may delete files during a sample.
            }
          } catch [IO.IOException] { } # A temporary subdirectory may also disappear.
          $visibleBytes = [Math]::Max($visibleBytes, $total)
          $walBytes = [Math]::Max($walBytes, $wal)
        }
      } catch [InvalidOperationException] { } # Child can exit during sampling.
      if ($watch.Elapsed.TotalSeconds -gt $TimeoutSeconds) {
        $timedOut = $true
        # Kill only this child and its descendants; never use process-name kills.
        & taskkill.exe /PID $process.Id /T /F | Out-Null
        break
      }
    } while (-not $process.WaitForExit(100))
    $process.WaitForExit()
    $exitCode = $process.ExitCode
    $output = $stdout.GetAwaiter().GetResult()
    $errors = $stderr.GetAwaiter().GetResult()
    [IO.File]::WriteAllText($Log, $output + "`n" + $errors, (New-Object Text.UTF8Encoding($false)))
  } catch {
    [IO.File]::WriteAllText($Log, $_.Exception.ToString(), (New-Object Text.UTF8Encoding($false)))
    $failure = @{status='process-error'; exitCode=$exitCode; timedOut=$timedOut; log=$Log; error=$_.Exception.Message}
    [IO.File]::WriteAllText("$Log.process.json", ($failure | ConvertTo-Json), (New-Object Text.UTF8Encoding($false)))
    throw
  } finally { $watch.Stop(); $process.Dispose() }
  $result = [pscustomobject]@{
    log = $Log; processSeconds = $watch.Elapsed.TotalSeconds
    exitCode = $exitCode; timedOut = $timedOut
    status = if ($timedOut) { 'timeout' } elseif ($exitCode -ne 0) { 'failed' } else { 'completed' }
    sampledPeakHandles = $handles; handleSamples = $observations
    processPeakWorkingSetBytes = $peakBytes
    samplingIntervalMs = 100; samplingScope = 'entire-child-including-oracle-and-cleanup'
    sampledVisibleDirectoryBytes = $visibleBytes; sampledDirectoryWalBytes = $walBytes
    directorySamplingScope = if ($ObservedDirectory) { 'visible files only; excludes anonymous SQLite sort files; includes setup and oracle' } else { 'not-observed' }
  }
  [IO.File]::WriteAllText("$Log.process.json", ($result | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding($false)))
  if ($timedOut) { throw "Benchmark process timed out; see $Log" }
  if ($exitCode -ne 0) { throw "Benchmark process failed (exit $exitCode); see $Log" }
  $result
}

function Get-ImportBenchmarkExecutable {
  param([string]$RepoRoot, [string]$Directory, [switch]$Release)
  $arguments = @('test', '--locked', '-p', 'app-services', '--test', 'import_benchmark',
    '--no-run', '--message-format=json', '--jobs', '2')
  if ($Release) { $arguments += '--release' }
  $log = Join-Path $Directory 'build.log'
  $null = Invoke-ImportBenchmarkProcess -Executable (Get-Command cargo -ErrorAction Stop).Source `
    -Arguments $arguments -WorkingDirectory $RepoRoot -Log $log
  $artifacts = @(foreach ($line in [IO.File]::ReadLines($log)) {
    if (-not $line.StartsWith('{')) { continue }
    $message = $line | ConvertFrom-Json
    if ($message.reason -eq 'compiler-artifact' -and $message.target.name -eq 'import_benchmark' -and
        $null -ne $message.executable) { $message.executable }
  })
  if ($artifacts.Count -ne 1 -or -not (Test-Path -LiteralPath $artifacts[0] -PathType Leaf)) {
    throw "Expected one Cargo compiler-artifact executable; see $log"
  }
  $artifacts[0]
}

function Read-ImportBenchmarkPayload {
  param([string]$Log)
  $prefix = '[import-benchmark] '
  $reports = @([IO.File]::ReadLines($Log) | Where-Object { $_.StartsWith($prefix) })
  if ($reports.Count -ne 1) { throw "Expected exactly one benchmark payload in $Log" }
  $reports[0].Substring($prefix.Length) | ConvertFrom-Json
}

Export-ModuleMember -Function Invoke-ImportBenchmarkProcess, Get-ImportBenchmarkExecutable, Read-ImportBenchmarkPayload
