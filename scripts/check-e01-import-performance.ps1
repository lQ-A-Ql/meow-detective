param(
  [string]$FixturePath = $env:FORENSICS_E01_FIXTURE,
  [int]$Runs = 3,
  [double]$MaxTotalMedianSeconds = 45.0,
  [double]$MaxEnumerationMedianSeconds = 30.0,
  [int]$MaxRssMb = 1024,
  [int]$MinRows = 90000,
  [int]$MinRowsPerSec = 6000,
  [string]$OutputDir = "",
  [string]$Cargo = "",
  [switch]$AllowSystemInfoWarnings
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($FixturePath)) {
  throw "Set FORENSICS_E01_FIXTURE or pass -FixturePath."
}
if ($Runs -lt 1) {
  throw "-Runs must be at least 1."
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$profileScriptPath = Join-Path $repoRoot "scripts/run-e01-import-profile.ps1"
if (-not (Test-Path -LiteralPath $profileScriptPath)) {
  throw "Missing profile runner: $profileScriptPath"
}

$fixture = Resolve-Path -LiteralPath $FixturePath -ErrorAction Stop
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $repoRoot "artifacts/import-performance-gates"
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

$profileArgs = @{
  FixturePath = $fixture.Path
  Runs = $Runs
  OutputDir = $OutputDir
}
if (-not [string]::IsNullOrWhiteSpace($Cargo)) {
  $profileArgs.Cargo = $Cargo
}

Write-Host "Running E01 import performance gate"
Write-Host "Fixture: $($fixture.Path)"
Write-Host "Runs: $Runs"
Write-Host "Thresholds: totalMedian<=${MaxTotalMedianSeconds}s enumerationMedian<=${MaxEnumerationMedianSeconds}s rss<=${MaxRssMb}MB rows>=${MinRows} rowsPerSec>=${MinRowsPerSec}"

$profileOutput = @(& $profileScriptPath @profileArgs 2>&1 | ForEach-Object { $_.ToString() })
$profileOutput | ForEach-Object { Write-Host $_ }

$jsonPath = $null
foreach ($line in $profileOutput) {
  if ($line -match '^JSON:\s*(?<path>.+)$') {
    $jsonPath = $Matches.path.Trim()
  }
}
if ([string]::IsNullOrWhiteSpace($jsonPath)) {
  $latestJson = Get-ChildItem -LiteralPath $OutputDir -Filter "e01-import-profile-*.json" |
    Where-Object { $_.Name -notlike "*.gate.json" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
  if ($null -ne $latestJson) {
    $jsonPath = $latestJson.FullName
  }
}
if ([string]::IsNullOrWhiteSpace($jsonPath) -or -not (Test-Path -LiteralPath $jsonPath)) {
  throw "Profile runner did not produce a JSON summary path."
}

$summary = Get-Content -LiteralPath $jsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
$results = @($summary.results)
if ($results.Count -ne $Runs) {
  throw "Expected $Runs profile result(s), got $($results.Count)."
}

if ($null -eq $summary.totalMedianSeconds) {
  throw "Missing totalMedianSeconds in profile summary."
}
if ([double]$summary.totalMedianSeconds -gt $MaxTotalMedianSeconds) {
  throw "Total median regression: $($summary.totalMedianSeconds)s > ${MaxTotalMedianSeconds}s"
}

if ($null -eq $summary.enumerationMedianSeconds) {
  throw "Missing enumerationMedianSeconds in profile summary."
}
if ([double]$summary.enumerationMedianSeconds -gt $MaxEnumerationMedianSeconds) {
  throw "Enumeration median regression: $($summary.enumerationMedianSeconds)s > ${MaxEnumerationMedianSeconds}s"
}

if ($null -eq $summary.rssMaxMb) {
  throw "Missing rssMaxMb in profile summary."
}
if ([double]$summary.rssMaxMb -gt $MaxRssMb) {
  throw "RSS regression: $($summary.rssMaxMb)MB > ${MaxRssMb}MB"
}

foreach ($run in $results) {
  if ($null -eq $run.rows -or [int64]$run.rows -lt $MinRows) {
    throw "Run $($run.run) imported too few rows: $($run.rows) < $MinRows"
  }
  if ($null -eq $run.rowsPerSec -or [int64]$run.rowsPerSec -lt $MinRowsPerSec) {
    throw "Run $($run.run) enumeration throughput too low: $($run.rowsPerSec) rows/s < $MinRowsPerSec"
  }
  if ($null -eq $run.log -or -not (Test-Path -LiteralPath $run.log)) {
    throw "Run $($run.run) raw log is missing: $($run.log)"
  }

  $log = Get-Content -LiteralPath $run.log -Raw -Encoding UTF8
  if ($log -notmatch 'NTFS shape: root Windows=\d+, root System32=0, key hives/logs=[1-9]\d*') {
    throw "Run $($run.run) did not prove the NTFS tree shape in the raw log."
  }
  if ($log -notmatch 'Timeline events after lazy query:\s*[1-9]\d*') {
    throw "Run $($run.run) did not prove lazy timeline projection in the raw log."
  }
  if ($log -notmatch 'System info: status=Parsed') {
    throw "Run $($run.run) did not parse system information."
  }
  if (-not $AllowSystemInfoWarnings -and $log -notmatch 'System info: status=Parsed,.*warnings=0') {
    throw "Run $($run.run) produced system information warnings."
  }
}

$gate = [ordered]@{
  generatedAt = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss zzz")
  fixture = $fixture.Path
  profileJson = $jsonPath
  runs = $Runs
  thresholds = [ordered]@{
    maxTotalMedianSeconds = $MaxTotalMedianSeconds
    maxEnumerationMedianSeconds = $MaxEnumerationMedianSeconds
    maxRssMb = $MaxRssMb
    minRows = $MinRows
    minRowsPerSec = $MinRowsPerSec
    allowSystemInfoWarnings = [bool]$AllowSystemInfoWarnings
  }
  observed = [ordered]@{
    totalMedianSeconds = $summary.totalMedianSeconds
    enumerationMedianSeconds = $summary.enumerationMedianSeconds
    rssMaxMb = $summary.rssMaxMb
  }
  passed = $true
}
$gatePath = [System.IO.Path]::ChangeExtension($jsonPath, ".gate.json")
$gate | ConvertTo-Json -Depth 8 | Out-File -LiteralPath $gatePath -Encoding UTF8

Write-Host "E01 import performance gate passed"
Write-Host "Gate JSON: $gatePath"
