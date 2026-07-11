param(
  [string]$WindowsFixturePath = $env:FORENSICS_STAGE2_WINDOWS_E01,
  [string]$LinuxFixturePath = $env:FORENSICS_STAGE2_LINUX_E01,
  [ValidateSet('both', 'windows-first', 'linux-first')]
  [string]$Order = 'both',
  [switch]$RequireFixtures
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$windowsMissing = [string]::IsNullOrWhiteSpace($WindowsFixturePath)
$linuxMissing = [string]::IsNullOrWhiteSpace($LinuxFixturePath)
if ($windowsMissing -and $linuxMissing) {
  if ($RequireFixtures) {
    throw 'Stage 2 real-sample isolation requires both WindowsFixturePath and LinuxFixturePath.'
  }
  Write-Host 'Stage 2 real-sample isolation skipped: no private fixture paths were supplied.'
  Write-Host 'Pass -RequireFixtures to turn a missing fixture into a gate failure.'
  return
}
if ($windowsMissing -or $linuxMissing) {
  throw 'Stage 2 real-sample isolation requires both fixture paths; only one was supplied.'
}

foreach ($fixture in @(
  @{ Label = 'Windows'; Path = $WindowsFixturePath },
  @{ Label = 'Linux'; Path = $LinuxFixturePath }
)) {
  if (-not (Test-Path -LiteralPath $fixture.Path -PathType Leaf)) {
    throw "$($fixture.Label) Stage 2 fixture is not a file: $($fixture.Path)"
  }
}

$resolvedWindows = (Resolve-Path -LiteralPath $WindowsFixturePath).Path
$resolvedLinux = (Resolve-Path -LiteralPath $LinuxFixturePath).Path
$testNames = switch ($Order) {
  'windows-first' { @('real_samples_import_into_isolated_source_databases_serially') }
  'linux-first' { @('real_samples_remain_isolated_when_linux_imports_first') }
  default {
    @(
      'real_samples_import_into_isolated_source_databases_serially',
      'real_samples_remain_isolated_when_linux_imports_first'
    )
  }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$previousWindows = $env:FORENSICS_STAGE2_WINDOWS_E01
$previousLinux = $env:FORENSICS_STAGE2_LINUX_E01
Push-Location $repoRoot
try {
  $env:FORENSICS_STAGE2_WINDOWS_E01 = $resolvedWindows
  $env:FORENSICS_STAGE2_LINUX_E01 = $resolvedLinux
  foreach ($testName in $testNames) {
    Write-Host "Running Stage 2 real-sample isolation: $testName"
    & cargo test -p forensics-desktop --test dual_source_import $testName -- --ignored --exact --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) {
      throw "Stage 2 real-sample isolation failed: $testName"
    }
  }
}
finally {
  $env:FORENSICS_STAGE2_WINDOWS_E01 = $previousWindows
  $env:FORENSICS_STAGE2_LINUX_E01 = $previousLinux
  Pop-Location
}

Write-Host "Stage 2 real-sample isolation passed: order=$Order"
