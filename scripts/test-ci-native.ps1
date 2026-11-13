#requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$scratch = Join-Path $root 'output/ci-runtime/runner-self-test'
New-Item -ItemType Directory -Path $scratch -Force | Out-Null
$runner = Join-Path $PSScriptRoot 'run-ci-native.ps1'
$hostExecutable = (Get-Process -Id $PID).Path
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
try {
  $env:TEMP = $scratch
  $env:TMP = $scratch
  $successLog = Join-Path $scratch 'success.log'
  & $runner -FilePath $hostExecutable -Arguments @('-NoProfile', '-Command', '[Console]::Out.WriteLine("success-output"); exit 0') -LogPath $successLog
  $success = Get-Content -LiteralPath $successLog -Raw
  if ($success -notmatch 'success-output' -or $success -notmatch 'NativeExitCode=0') {
    throw 'CI command runner did not preserve successful output and exit status'
  }
  $failureLog = Join-Path $scratch 'failure.log'
  $failed = $false
  try {
    & $runner -FilePath $hostExecutable -Arguments @('-NoProfile', '-Command', '[Console]::Out.WriteLine("failure-output"); [Console]::Error.WriteLine("failure-error"); exit 7') -LogPath $failureLog
  } catch {
    if ($_.Exception.Message -notmatch 'failed with exit code 7') { throw }
    $failed = $true
  }
  if (-not $failed) { throw 'CI command runner accepted a failed native command' }
  $failure = Get-Content -LiteralPath $failureLog -Raw
  foreach ($expected in @('failure-output', 'failure-error', 'NativeExitCode=7')) {
    if ($failure -notmatch [regex]::Escape($expected)) {
      throw "CI command runner lost failure evidence: $expected"
    }
  }
  Write-Host 'CI native command runner self-test passed'
} finally {
  $env:TEMP = $previousTemp
  $env:TMP = $previousTmp
}
exit 0
