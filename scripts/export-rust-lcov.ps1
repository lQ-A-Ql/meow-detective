param(
  [string]$TargetDir = 'target/llvm-cov-target',
  [string]$OutputPath = 'rust-lcov.info',
  [string]$BuildProfile = 'debug'
)

# Mirrors `cargo llvm-cov report --lcov` but passes -object arguments through a response file,
# because the workspace test binary list exceeds the Windows 32,767-character command-line limit.

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

function Resolve-LlvmTool([string]$Name) {
  $sysroot = (& rustc --print sysroot).Trim()
  $hostLine = (& rustc -vV) | Where-Object { $_ -like 'host: *' } | Select-Object -First 1
  $toolBase = Join-Path $sysroot "lib/rustlib/$($hostLine.Substring(6).Trim())/bin/$Name"
  foreach ($candidate in @("$toolBase.exe", $toolBase)) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
  }
  throw "$Name was not found; install it with: rustup component add llvm-tools-preview"
}

function Invoke-Tool([string]$FilePath, [string[]]$Arguments, [string]$StdoutPath) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $FilePath
  $psi.Arguments = ($Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = [bool]$StdoutPath
  $process = [System.Diagnostics.Process]::Start($psi)
  if ($StdoutPath) {
    $stream = [System.IO.File]::Create($StdoutPath)
    try { $process.StandardOutput.BaseStream.CopyTo($stream) } finally { $stream.Dispose() }
  }
  $process.WaitForExit()
  if ($process.ExitCode -ne 0) {
    throw "$([System.IO.Path]::GetFileName($FilePath)) exited with code $($process.ExitCode)"
  }
}

$utf8 = New-Object System.Text.UTF8Encoding($false)
$targetRoot = (Resolve-Path -LiteralPath $TargetDir).Path
$metadata = (& cargo metadata --no-deps --format-version 1) | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }

$profraw = @(Get-ChildItem -LiteralPath $targetRoot -Filter '*.profraw' -File)
if ($profraw.Count -eq 0) { throw "No *.profraw files found in $targetRoot; run cargo llvm-cov --no-report first" }
$profrawList = Join-Path $targetRoot 'meow-detective-profraw-list'
[System.IO.File]::WriteAllLines($profrawList, [string[]]($profraw | ForEach-Object FullName), $utf8)
$profdata = Join-Path $targetRoot 'meow-detective.profdata'
Invoke-Tool (Resolve-LlvmTool 'llvm-profdata') @('merge', '-sparse', '-f', $profrawList, '-o', $profdata)

$targetNames = New-Object System.Collections.Generic.SortedSet[string]
foreach ($package in $metadata.packages) {
  [void]$targetNames.Add($package.name)
  foreach ($target in $package.targets) { [void]$targetNames.Add($target.name) }
}
$namePattern = '^(lib)?(' + (($targetNames | ForEach-Object { [regex]::Escape($_).Replace('-', '(-|_)') }) -join '|') + ')(-[0-9a-f]+)?$'

$profileRoot = Join-Path $targetRoot $BuildProfile
$skippedDirs = '\\(incremental|\.fingerprint|fingerprint|build)(\\|$)'
$objects = @(Get-ChildItem -LiteralPath $profileRoot -Recurse -File |
    Where-Object {
      $_.Extension -in '.exe', '.dll' -and
      $_.DirectoryName.Substring($profileRoot.Length) -notmatch $skippedDirs -and
      $_.Name.Split('.')[0] -match $namePattern
    } |
    Sort-Object FullName)
if ($objects.Count -eq 0) { throw "No instrumented workspace objects found under $profileRoot" }
$objectArgs = Join-Path $targetRoot 'meow-detective-objects.rsp'
[System.IO.File]::WriteAllLines($objectArgs, [string[]]($objects | ForEach-Object { "-object `"$($_.FullName)`"" }), $utf8)

$homeDir = [Environment]::GetFolderPath('UserProfile')
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $homeDir '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $homeDir '.rustup' }
$workspace = [regex]::Escape(($metadata.workspace_root).TrimEnd('\'))
$ignorePattern = @(
  '\\rustc\\([0-9a-f]+|[0-9]+\.[0-9]+\.[0-9]+)\\',
  "^$workspace(\\.*)?\\(tests|examples|benches)\\",
  "^$workspace(\\.*)?\\(tests\.rs|[0-9a-zA-Z_-]+[_-]tests\.rs)$",
  "^$([regex]::Escape($targetRoot.TrimEnd('\')))($|\\)",
  "^$([regex]::Escape($cargoHome.TrimEnd('\')))\\(registry|git)\\",
  "^$([regex]::Escape((Join-Path $rustupHome 'toolchains')))($|\\)"
) -join '|'

$outputFull = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($repoRoot, $OutputPath))
Write-Host "Exporting lcov from $($objects.Count) objects and $($profraw.Count) profiles to $outputFull"
Invoke-Tool (Resolve-LlvmTool 'llvm-cov') @(
  'export', '-format=lcov', "-instr-profile=$profdata", '-ignore-filename-regex', $ignorePattern, "@$objectArgs"
) $outputFull
