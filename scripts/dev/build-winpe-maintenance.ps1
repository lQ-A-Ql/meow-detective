# Builds the WinPE maintenance helper with a static CRT so it runs inside a
# stock WinPE image without the Visual C++ redistributable, and copies the
# binary next to the desktop application for the emulation maintenance CD.
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$vcvars = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path $vcvars)) {
    $vcvars = 'C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
}

$env:RUSTFLAGS = '-C target-feature=+crt-static'
$build = "call `"$vcvars`" >nul 2>&1 && cd /d $repoRoot && cargo build --release -p winpe-maintenance"
cmd.exe /d /s /c $build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:RUSTFLAGS = ''

$source = Join-Path $repoRoot 'target\release\meow-winpe-maintenance.exe'
$targetDir = Join-Path $repoRoot 'apps\desktop\src-tauri\resources\tools'
New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
Copy-Item -Force $source (Join-Path $targetDir 'meow-winpe-maintenance.exe')
Write-Host "maintenance tool copied to $targetDir"
