param(
  [string]$FixturePath = $env:FORENSICS_E01_FIXTURE,
  [int]$Runs = 3,
  [string]$OutputDir = "",
  [string]$Cargo = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($FixturePath)) {
  throw "Set FORENSICS_E01_FIXTURE or pass -FixturePath."
}
if ([string]::IsNullOrWhiteSpace($Cargo)) {
  if ($env:CARGO) {
    $Cargo = $env:CARGO
  } else {
    $Cargo = "cargo"
  }
}

$fixture = Resolve-Path -LiteralPath $FixturePath -ErrorAction Stop
if ($Runs -lt 1) {
  throw "-Runs must be at least 1."
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $repoRoot "artifacts/import-profiles"
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$summaryPath = Join-Path $OutputDir "e01-import-profile-$timestamp.md"
$jsonPath = Join-Path $OutputDir "e01-import-profile-$timestamp.json"

function Convert-ProfileLine {
  param([Parameter(Mandatory = $true)][string]$Line)

  if ($Line -notmatch '^\[import-profile\]\s+(?<progress>\d+)%\s+(?<detail>.+)$') {
    return $null
  }

  $detail = $Matches.detail
  $record = [ordered]@{
    progress = [int]$Matches.progress
    detail = $detail
  }

  foreach ($match in [regex]::Matches($detail, '(?<key>[A-Za-z][A-Za-z0-9_-]*)=(?<value>[^\s,]+)')) {
    $key = $match.Groups['key'].Value
    $value = $match.Groups['value'].Value
    if ($value -match '^-?\d+$') {
      $record[$key] = [int64]$value
    } elseif ($value -match '^-?\d+\.\d+$') {
      $record[$key] = [double]$value
    } else {
      $record[$key] = $value
    }
  }

  return [pscustomobject]$record
}

function Get-ProfilePhase {
  param(
    [Parameter(Mandatory = $true)][object[]]$Records,
    [Parameter(Mandatory = $true)][string]$Phase
  )

  $matches = @($Records | Where-Object {
      $_.PSObject.Properties.Name -contains "phase" -and $_.phase -eq $Phase
    })
  if ($matches.Count -eq 0) {
    return $null
  }
  $timedMatches = @($matches | Where-Object {
      $_.PSObject.Properties.Name -contains "elapsedMs"
    })
  if ($timedMatches.Count -gt 0) {
    return $timedMatches[-1]
  }
  return $matches[-1]
}

function Get-Median {
  param([double[]]$Values)

  if ($Values.Count -eq 0) {
    return $null
  }
  $sorted = @($Values | Sort-Object)
  $middle = [int][Math]::Floor($sorted.Count / 2)
  if (($sorted.Count % 2) -eq 1) {
    return $sorted[$middle]
  }
  return ($sorted[$middle - 1] + $sorted[$middle]) / 2.0
}

function Convert-MillisToSeconds {
  param($Value)

  if ($null -eq $Value) {
    return 0.0
  }
  return [double]$Value / 1000.0
}

function Invoke-E01ImportProfileTest {
  param(
    [Parameter(Mandatory = $true)][string]$Cargo,
    [Parameter(Mandatory = $true)][string]$WorkingDirectory,
    [Parameter(Mandatory = $true)][string]$FixturePath
  )

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Cargo
  $psi.Arguments = "test -p app-services e01_full_import -- --ignored --nocapture"
  $psi.WorkingDirectory = $WorkingDirectory
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.EnvironmentVariables["FORENSICS_E01_FIXTURE"] = $FixturePath

  $process = New-Object System.Diagnostics.Process
  $process.StartInfo = $psi
  [void]$process.Start()
  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  $process.WaitForExit()
  $stdout = $stdoutTask.GetAwaiter().GetResult()
  $stderr = $stderrTask.GetAwaiter().GetResult()

  $lines = New-Object System.Collections.Generic.List[string]
  if (-not [string]::IsNullOrEmpty($stdout)) {
    $stdout -split "`r?`n" | ForEach-Object {
      if (-not [string]::IsNullOrEmpty($_)) {
        $lines.Add($_) | Out-Null
      }
    }
  }
  if (-not [string]::IsNullOrEmpty($stderr)) {
    $stderr -split "`r?`n" | ForEach-Object {
      if (-not [string]::IsNullOrEmpty($_)) {
        $lines.Add($_) | Out-Null
      }
    }
  }

  return [pscustomobject]@{
    ExitCode = $process.ExitCode
    Lines = @($lines)
  }
}

$results = New-Object System.Collections.Generic.List[object]

Push-Location $repoRoot
try {
  for ($i = 1; $i -le $Runs; $i++) {
    Write-Host "[$i/$Runs] Running real E01 import profile against $fixture"
    $commandResult = Invoke-E01ImportProfileTest `
      -Cargo $Cargo `
      -WorkingDirectory $repoRoot.Path `
      -FixturePath $fixture.Path
    $output = @($commandResult.Lines)
    $exitCode = $commandResult.ExitCode
    $rawLogPath = Join-Path $OutputDir "e01-import-profile-$timestamp-run-$i.log"
    $output | Out-File -LiteralPath $rawLogPath -Encoding UTF8

    if ($exitCode -ne 0) {
      throw "Run $i failed with exit code $exitCode. Log: $rawLogPath"
    }

    $records = @($output | ForEach-Object { Convert-ProfileLine -Line $_ } | Where-Object { $null -ne $_ })
    $total = Get-ProfilePhase -Records $records -Phase "total"
    $probe = Get-ProfilePhase -Records $records -Phase "probe"
    if ($null -eq $probe) {
      $probe = Get-ProfilePhase -Records $records -Phase "probe-resume"
    }
    $readerBuild = Get-ProfilePhase -Records $records -Phase "reader-build"
    $enumeration = Get-ProfilePhase -Records $records -Phase "enumeration"
    $enumMerge = Get-ProfilePhase -Records $records -Phase "enum-merge"
    $postImport = Get-ProfilePhase -Records $records -Phase "post-import"
    if ($null -eq $postImport) {
      $postImport = Get-ProfilePhase -Records $records -Phase "post-import-skip"
    }

    $run = [ordered]@{
      run = $i
      log = $rawLogPath
      totalMs = if ($total) { $total.elapsedMs } else { $null }
      probeMs = if ($probe) { $probe.elapsedMs } else { $null }
      readerBuildMs = if ($readerBuild) { $readerBuild.elapsedMs } else { $null }
      enumerationMs = if ($enumeration) { $enumeration.elapsedMs } else { $null }
      enumMergeMs = if ($enumMerge) { $enumMerge.elapsedMs } else { $null }
      postImportMs = if ($postImport) { $postImport.elapsedMs } else { $null }
      rows = if ($enumeration -and ($enumeration.PSObject.Properties.Name -contains "rows")) { $enumeration.rows } else { $null }
      rowsPerSec = if ($enumeration -and ($enumeration.PSObject.Properties.Name -contains "rowsPerSec")) { $enumeration.rowsPerSec } else { $null }
      dataMb = if ($enumeration -and ($enumeration.PSObject.Properties.Name -contains "dataMb")) { $enumeration.dataMb } else { $null }
      mbPerSec = if ($enumeration -and ($enumeration.PSObject.Properties.Name -contains "mbPerSec")) { $enumeration.mbPerSec } else { $null }
      rssMb = if ($records.Count -gt 0) {
        $recordRss = @($records | Where-Object {
            $_.PSObject.Properties.Name -contains "rssMb" -and $null -ne $_.rssMb
          } | ForEach-Object { [double]$_.rssMb })
        if ($recordRss.Count -gt 0) {
          ($recordRss | Measure-Object -Maximum).Maximum
        } else {
          $null
        }
      } else {
        $null
      }
      records = $records
    }
    $results.Add([pscustomobject]$run) | Out-Null
  }
} finally {
  Pop-Location
}

$totalSeconds = @($results | Where-Object { $null -ne $_.totalMs } | ForEach-Object { [double]$_.totalMs / 1000.0 })
$enumerationSeconds = @($results | Where-Object { $null -ne $_.enumerationMs } | ForEach-Object { [double]$_.enumerationMs / 1000.0 })
$rssValues = @($results | Where-Object { $null -ne $_.rssMb } | ForEach-Object { [double]$_.rssMb })

$summary = [ordered]@{
  fixture = $fixture.Path
  generatedAt = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss zzz")
  runs = $Runs
  totalMedianSeconds = Get-Median -Values $totalSeconds
  enumerationMedianSeconds = Get-Median -Values $enumerationSeconds
  rssMaxMb = if ($rssValues.Count -gt 0) { ($rssValues | Measure-Object -Maximum).Maximum } else { $null }
  results = $results
}

$summary | ConvertTo-Json -Depth 8 | Out-File -LiteralPath $jsonPath -Encoding UTF8

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# E01 Import Profile $timestamp") | Out-Null
$lines.Add("") | Out-Null
$lines.Add("- Fixture: ``$($fixture.Path)``") | Out-Null
$lines.Add("- Generated: $($summary.generatedAt)") | Out-Null
$lines.Add("- Runs: $Runs") | Out-Null
$lines.Add("- Total median: $([Math]::Round([double]$summary.totalMedianSeconds, 1))s") | Out-Null
$lines.Add("- Enumeration median: $([Math]::Round([double]$summary.enumerationMedianSeconds, 1))s") | Out-Null
$lines.Add("- RSS max: $($summary.rssMaxMb)MB") | Out-Null
$lines.Add("") | Out-Null
$lines.Add("| Run | Total | Probe | Reader build | Enumeration | Enum merge | Post-import | Rows | Rows/s | MB/s | RSS | Log |") | Out-Null
$lines.Add("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |") | Out-Null
foreach ($run in $results) {
  $lines.Add((
      "| {0} | {1:n1}s | {2:n1}s | {3:n1}s | {4:n1}s | {5:n1}s | {6:n1}s | {7} | {8} | {9} | {10}MB | {11} |" -f
      $run.run,
      (Convert-MillisToSeconds -Value $run.totalMs),
      (Convert-MillisToSeconds -Value $run.probeMs),
      (Convert-MillisToSeconds -Value $run.readerBuildMs),
      (Convert-MillisToSeconds -Value $run.enumerationMs),
      (Convert-MillisToSeconds -Value $run.enumMergeMs),
      (Convert-MillisToSeconds -Value $run.postImportMs),
      $run.rows,
      $run.rowsPerSec,
      $run.mbPerSec,
      $run.rssMb,
      (Split-Path -Leaf $run.log)
    )) | Out-Null
}
$lines | Out-File -LiteralPath $summaryPath -Encoding UTF8

Write-Host "E01 import profile complete"
Write-Host "Markdown: $summaryPath"
Write-Host "JSON: $jsonPath"
