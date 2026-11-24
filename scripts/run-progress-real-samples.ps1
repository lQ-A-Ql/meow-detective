#Requires -Version 7.0
param(
    [Parameter(Mandatory = $true)][string]$TestBinary,
    [string[]]$WindowsFixtures = @(),
    [string[]]$LinuxFixtures = @(),
    [ValidateRange(1, 15)][int]$TimeoutMinutes = 15,
    [string]$OutputRoot = "artifacts/task-progress-acceptance"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$binary = (Resolve-Path -LiteralPath $TestBinary).Path
$root = [IO.Path]::GetFullPath((Join-Path $repository $OutputRoot))
New-Item -ItemType Directory -Force -Path $root | Out-Null
$runRoot = Join-Path $root ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $runRoot | Out-Null
$results = [System.Collections.Generic.List[object]]::new()
$savedTemp = $env:TEMP
$savedTmp = $env:TMP
$savedOutput = $env:FORENSICS_PERF_OUTPUT
$savedWindows = $env:FORENSICS_PERF_WINDOWS_E01
$savedLinux = $env:FORENSICS_PERF_LINUX_E01

try {
    foreach ($platform in @('windows', 'linux')) {
        $fixtures = if ($platform -eq 'windows') { $WindowsFixtures } else { $LinuxFixtures }
        $index = 0
        foreach ($fixture in $fixtures) {
            $index += 1
            $path = (Resolve-Path -LiteralPath $fixture).Path
            $label = "$platform-$index"
            $output = Join-Path $runRoot $label
            $scratch = Join-Path $output 'scratch'
            New-Item -ItemType Directory -Force -Path $scratch | Out-Null
            $env:TEMP = $scratch
            $env:TMP = $scratch
            $env:FORENSICS_PERF_OUTPUT = $output
            if ($platform -eq 'windows') { $env:FORENSICS_PERF_WINDOWS_E01 = $path }
            else { $env:FORENSICS_PERF_LINUX_E01 = $path }
            Write-Output "SAMPLE_START $label $output"
            $started = [DateTime]::UtcNow
            $clock = [Diagnostics.Stopwatch]::StartNew()
            $process = Start-Process -FilePath $binary -WorkingDirectory $repository `
                -ArgumentList @("${platform}_real_sample_performance_acceptance", '--ignored', '--exact', '--nocapture', '--test-threads=1') `
                -RedirectStandardOutput (Join-Path $output 'stdout.log') `
                -RedirectStandardError (Join-Path $output 'stderr.log') `
                -WindowStyle Hidden -PassThru
            $null = $process.Handle
            $remaining = [Math]::Max(0, $TimeoutMinutes * 60000 - $clock.ElapsedMilliseconds)
            $timedOut = -not $process.WaitForExit([int]$remaining)
            if ($timedOut) {
                $process.Kill($true)
                if (-not $process.WaitForExit(10000)) { throw "Sample process did not terminate after its deadline" }
            }
            $process.Refresh()
            $reports = @(Get-ChildItem -LiteralPath $output -Recurse -Filter 'acceptance.json' -File)
            $acceptance = if (-not $timedOut -and $process.ExitCode -eq 0 -and $reports.Count -eq 1) {
                Get-Content -LiteralPath $reports[0].FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            } else { $null }
            $status = if ($timedOut) { 'timedOut' }
                elseif ($process.ExitCode -eq 0 -and $null -ne $acceptance) { $acceptance.status }
                else { 'failed' }
            $results.Add([ordered]@{
                label = $label
                platform = $platform
                fixture = $path
                output = $output
                startedUtc = $started.ToString('o')
                finishedUtc = [DateTime]::UtcNow.ToString('o')
                exitCode = $process.ExitCode
                timeoutMinutes = $TimeoutMinutes
                elapsedSeconds = $clock.Elapsed.TotalSeconds
                status = $status
            })
            if ($timedOut) {
                [ordered]@{label=$label;status=$status;timeoutMinutes=$TimeoutMinutes;
                    elapsedSeconds=$clock.Elapsed.TotalSeconds;
                    lastProgress=Get-Content -LiteralPath (Join-Path $output 'stderr.log') -Tail 5
                } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'deadline-result.json') -Encoding UTF8
            }
            ConvertTo-Json -InputObject $results.ToArray() -Depth 8 | Set-Content -LiteralPath (Join-Path $runRoot 'matrix.json') -Encoding UTF8
            Write-Output "SAMPLE_FINISHED $label $status"
        }
    }
} finally {
    $env:TEMP = $savedTemp
    $env:TMP = $savedTmp
    $env:FORENSICS_PERF_OUTPUT = $savedOutput
    $env:FORENSICS_PERF_WINDOWS_E01 = $savedWindows
    $env:FORENSICS_PERF_LINUX_E01 = $savedLinux
}

Write-Output "SAMPLE_MATRIX $runRoot"
if ($results.Count -eq 0 -or @($results | Where-Object { $_.status -in @('failed', 'timedOut') }).Count -gt 0) {
    exit 1
}
