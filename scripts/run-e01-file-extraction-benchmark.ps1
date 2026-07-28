[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Windows1FixturePath,

  [Parameter(Mandatory = $true)]
  [string]$Windows2FixturePath,

  [Parameter(Mandatory = $true)]
  [string]$Linux1FixturePath,

  [Parameter(Mandatory = $true)]
  [string]$Linux2FixturePath,

  [string]$OutputPath = "artifacts/file-extraction-benchmark/file-extraction-benchmark.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$invariantCulture = [System.Globalization.CultureInfo]::InvariantCulture
$benchmarkMarker = "FILE_EXTRACTION_BENCHMARK_JSON="
$fixtureEnvironment = [ordered]@{
  FORENSICS_WINDOWS1_E01_FIXTURE = $Windows1FixturePath
  FORENSICS_WINDOWS2_E01_FIXTURE = $Windows2FixturePath
  FORENSICS_LINUX1_E01_FIXTURE = $Linux1FixturePath
  FORENSICS_LINUX2_E01_FIXTURE = $Linux2FixturePath
}
$sampleLabels = @{
  FORENSICS_WINDOWS1_E01_FIXTURE = "Windows 1"
  FORENSICS_WINDOWS2_E01_FIXTURE = "Windows 2"
  FORENSICS_LINUX1_E01_FIXTURE = "Linux 1"
  FORENSICS_LINUX2_E01_FIXTURE = "Linux 2"
}

function Resolve-FixtureFile {
  param([string]$Path)

  $resolved = Resolve-Path -LiteralPath $Path -ErrorAction Stop
  if (-not (Test-Path -LiteralPath $resolved.Path -PathType Leaf)) {
    throw "Fixture is not a file: $Path"
  }
  return $resolved.Path
}

function Format-Decimal {
  param(
    [double]$Value,
    [string]$Format = "0.000"
  )

  return $Value.ToString($Format, $invariantCulture)
}

function Get-StorageSummary {
  param([string[]]$Paths)

  $summaries = @()
  $roots = @($Paths | ForEach-Object { [System.IO.Path]::GetPathRoot($_) } | Sort-Object -Unique)
  foreach ($root in $roots) {
    try {
      $driveLetter = $root.Substring(0, 1)
      $volume = Get-Volume -DriveLetter $driveLetter -ErrorAction Stop
      $disk = Get-Partition -DriveLetter $driveLetter -ErrorAction Stop |
        Get-Disk -ErrorAction Stop
      $summaries += "${driveLetter}: $($volume.FileSystem) on $($disk.FriendlyName) ($($disk.BusType))"
    } catch {
      $summaries += "$root storage details unavailable"
    }
  }
  return $summaries -join "; "
}

foreach ($name in @($fixtureEnvironment.Keys)) {
  $fixtureEnvironment[$name] = Resolve-FixtureFile $fixtureEnvironment[$name]
}

foreach ($requiredSecret in @(
  "FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD",
  "FORENSICS_BITLOCKER_PRIVATE_JC2_RECOVERY_PASSWORD"
)) {
  if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($requiredSecret, "Process"))) {
    throw "Set $requiredSecret before running the four-sample extraction benchmark."
  }
}

$resolvedOutput = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
  [System.IO.Path]::GetFullPath($OutputPath)
} else {
  [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputPath))
}
$outputDirectory = Split-Path -Parent $resolvedOutput
[System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
$logDirectory = Join-Path $repoRoot "artifacts/file-extraction-benchmark"
[System.IO.Directory]::CreateDirectory($logDirectory) | Out-Null
$logPath = Join-Path $logDirectory "cargo-test.log"

$managedEnvironment = @($fixtureEnvironment.Keys) + "FORENSICS_E01_EXTRACTION_BENCHMARK_ONLY"
$previousEnvironment = @{}
foreach ($name in $managedEnvironment) {
  $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}

$commandOutput = @()
$exitCode = 1
try {
  foreach ($entry in $fixtureEnvironment.GetEnumerator()) {
    [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, "Process")
  }
  [Environment]::SetEnvironmentVariable(
    "FORENSICS_E01_EXTRACTION_BENCHMARK_ONLY",
    "1",
    "Process"
  )

  Push-Location $repoRoot
  try {
    $savedErrorActionPreference = $ErrorActionPreference
    try {
      # Windows PowerShell surfaces a native process' stderr as non-terminating
      # ErrorRecord values even when the process exits successfully.
      $ErrorActionPreference = "Continue"
      $commandOutput = @(
        & cargo test --release -p app-services --test e01_catalog_integrity catalog_integrity -- `
          --ignored --nocapture --test-threads=1 2>&1 | Tee-Object -FilePath $logPath
      )
      $exitCode = $LASTEXITCODE
    } finally {
      $ErrorActionPreference = $savedErrorActionPreference
    }
  } finally {
    Pop-Location
  }
} finally {
  foreach ($name in $managedEnvironment) {
    [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], "Process")
  }
}

$records = @(
  foreach ($line in $commandOutput) {
    $text = $line.ToString()
    $markerIndex = $text.IndexOf($benchmarkMarker, [System.StringComparison]::Ordinal)
    if ($markerIndex -ge 0) {
      $json = $text.Substring($markerIndex + $benchmarkMarker.Length)
      $json | ConvertFrom-Json
    }
  }
)

if ($records.Count -ne 4) {
  throw "Expected four extraction benchmark records, found $($records.Count). See $logPath."
}

$gitCommit = (& git -C $repoRoot rev-parse --short=12 HEAD).Trim()
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$operatingSystem = Get-CimInstance Win32_OperatingSystem
$computer = Get-CimInstance Win32_ComputerSystem
$memoryGiB = Format-Decimal ([double]$computer.TotalPhysicalMemory / 1GB) "0.00"
$storageSummary = Get-StorageSummary (
  @($fixtureEnvironment.Values) + [System.IO.Path]::GetTempPath()
)
$builder = [System.Text.StringBuilder]::new()
[void]$builder.AppendLine("# Four-sample E01 file extraction benchmark")
[void]$builder.AppendLine()
[void]$builder.AppendLine("- Date: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')")
[void]$builder.AppendLine("- Commit: ``$gitCommit``")
[void]$builder.AppendLine("- Build profile: Rust ``release``")
[void]$builder.AppendLine("- Host: $($cpu.Name.Trim()), $($cpu.NumberOfCores) cores / $($cpu.NumberOfLogicalProcessors) logical processors, $memoryGiB GiB RAM")
[void]$builder.AppendLine("- OS: $($operatingSystem.Caption) $($operatingSystem.Version) build $($operatingSystem.BuildNumber)")
[void]$builder.AppendLine("- Storage: $storageSummary")
[void]$builder.AppendLine("- Mode: post-enumeration warm extraction, one successful 128-512 MiB file per sample")
[void]$builder.AppendLine("- Destination: system temporary directory; SHA-256, flush, sync, and atomic publish are included")
[void]$builder.AppendLine("- Scheduling: samples and exports are serial; import/enumeration time is excluded from the reported extraction phases")
[void]$builder.AppendLine()
[void]$builder.AppendLine("| Sample | Image | Image GiB | Internal file | Partition | BitLocker | File MiB | Prepare s | Copy s | Finalize s | Total s | Copy MiB/s | Total MiB/s | Progress events |")
[void]$builder.AppendLine("|---|---|---:|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|")

foreach ($sampleEnvironment in $fixtureEnvironment.Keys) {
  $record = @($records | Where-Object { $_.sample -eq $sampleEnvironment })
  if ($record.Count -ne 1) {
    throw "Expected one benchmark record for $sampleEnvironment, found $($record.Count)."
  }
  $record = $record[0]
  $fixture = Get-Item -LiteralPath $fixtureEnvironment[$sampleEnvironment]
  $imageGiB = Format-Decimal ($fixture.Length / 1GB)
  $fileMiB = Format-Decimal ([double]$record.bytes / 1MB)
  $prepareSeconds = Format-Decimal ([double]$record.prepareMs / 1000.0)
  $copySeconds = Format-Decimal ([double]$record.copyMs / 1000.0)
  $finalizeSeconds = Format-Decimal ([double]$record.finalizingMs / 1000.0)
  $totalSeconds = Format-Decimal ([double]$record.totalMs / 1000.0)
  $copyRate = Format-Decimal ([double]$record.copyMiBPerSecond)
  $totalRate = Format-Decimal ([double]$record.totalMiBPerSecond)
  $internalPath = $record.path.ToString().Replace("|", "\|").Replace([char]96, [char]39)
  $bitlocker = if ([bool]$record.bitlocker) { "yes" } else { "no" }
  [void]$builder.AppendLine(
    "| $($sampleLabels[$sampleEnvironment]) | $($fixture.Name) | $imageGiB | ``$internalPath`` | $($record.partitionIndex) | $bitlocker | $fileMiB | $prepareSeconds | $copySeconds | $finalizeSeconds | $totalSeconds | $copyRate | $totalRate | $($record.progressEvents) |"
  )
}

[void]$builder.AppendLine()
[void]$builder.AppendLine("## Findings")
[void]$builder.AppendLine()
$copyRates = @($records | ForEach-Object { [double]$_.copyMiBPerSecond })
$totalRates = @($records | ForEach-Object { [double]$_.totalMiBPerSecond })
$prepareTimes = @($records | ForEach-Object { [double]$_.prepareMs / 1000.0 })
$finalizeTimes = @($records | ForEach-Object { [double]$_.finalizingMs / 1000.0 })
$copyMinimum = Format-Decimal (($copyRates | Measure-Object -Minimum).Minimum)
$copyMaximum = Format-Decimal (($copyRates | Measure-Object -Maximum).Maximum)
$totalMinimum = Format-Decimal (($totalRates | Measure-Object -Minimum).Minimum)
$totalMaximum = Format-Decimal (($totalRates | Measure-Object -Maximum).Maximum)
$prepareMinimum = Format-Decimal (($prepareTimes | Measure-Object -Minimum).Minimum)
$prepareMaximum = Format-Decimal (($prepareTimes | Measure-Object -Maximum).Maximum)
$finalizeMinimum = Format-Decimal (($finalizeTimes | Measure-Object -Minimum).Minimum)
$finalizeMaximum = Format-Decimal (($finalizeTimes | Measure-Object -Maximum).Maximum)
[void]$builder.AppendLine("- Warm-cache copy throughput spans $copyMinimum-$copyMaximum MiB/s across the four source and filesystem combinations.")
[void]$builder.AppendLine("- End-to-end throughput is $totalMinimum-$totalMaximum MiB/s; reader/filesystem preparation takes $prepareMinimum-$prepareMaximum s and durable finalization takes $finalizeMinimum-$finalizeMaximum s per extraction.")
[void]$builder.AppendLine("- Copy throughput can exceed sustained physical-device throughput because enumeration and candidate preview warm the Windows page cache. These values must not be treated as cold-cache or long-duration sequential-I/O limits.")
[void]$builder.AppendLine("- Windows 1 exercises a BitLocker-backed file. Windows 2 falls back to a non-BitLocker file because its unlocked BitLocker catalog has no regular file in the 128-512 MiB benchmark tier.")
[void]$builder.AppendLine()
[void]$builder.AppendLine("## Interpretation boundary")
[void]$builder.AppendLine()
[void]$builder.AppendLine("This is a single-run, post-enumeration warm-cache private-sample baseline, not a release threshold. It measures the production evidence reader, SHA-256 calculation, destination write, durable sync, and atomic publish. It does not include image import or filesystem enumeration. Cold-cache runs, repeated-run p50/p95, sustained multi-GiB exports, and RSS/CPU telemetry remain separate follow-up work.")
[void]$builder.AppendLine()
[void]$builder.AppendLine("Raw cargo output: ``artifacts/file-extraction-benchmark/cargo-test.log``")

$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($resolvedOutput, $builder.ToString(), $utf8NoBom)

if ($exitCode -ne 0) {
  throw "Extraction benchmark tests failed with exit code $exitCode. See $logPath."
}

Write-Host "Extraction benchmark report: $resolvedOutput"
