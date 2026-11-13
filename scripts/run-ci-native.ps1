#requires -Version 7.0
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$FilePath,
  [string[]]$Arguments = @(),
  [Parameter(Mandatory = $true)][string]$LogPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$command = Get-Command -Name $FilePath -CommandType Application -ErrorAction Stop | Select-Object -First 1
$resolvedLog = [System.IO.Path]::GetFullPath($LogPath)
New-Item -ItemType Directory -Path (Split-Path -Parent $resolvedLog) -Force | Out-Null
"Command: $($command.Name)" | Set-Content -LiteralPath $resolvedLog -Encoding utf8
& $command.Source @Arguments 2>&1 | Tee-Object -FilePath $resolvedLog -Append -ErrorAction Stop
$nativeExitCode = $LASTEXITCODE
"NativeExitCode=$nativeExitCode" | Add-Content -LiteralPath $resolvedLog -Encoding utf8
if ($nativeExitCode -ne 0) {
  throw "$($command.Name) failed with exit code $nativeExitCode; log: $resolvedLog"
}
