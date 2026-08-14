# Builds the standalone parser-plugin workspace (plugins-src/, plugin system
# M4) in release mode and stages each plugin DLL into the exe-adjacent layout
# the host plugin_loader scans: plugins/<evidence-platform>/ next to the
# executable (crates/app-services/src/plugin_loader/directory.rs).
#
# Outputs:
#   target/release/plugins/<platform>/*.dll                       (repo-root cargo build -p forensics-desktop layout)
#   apps/desktop/src-tauri/target/release/plugins/<platform>/*.dll (cargo tauri build layout, only when that
#                                                                 target directory already exists)
#
# The script is idempotent: re-running it overwrites the staged DLLs with the
# fresh build and produces the same layout every time.
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$vcvars = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path $vcvars)) {
    $vcvars = 'C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
}

# Plugin staging manifest. The platform of each entry must mirror the
# evidence_platform the plugin declares from meow_plugin_info
# (plugins-src/<name>/src/lib.rs); the loader scans plugins/windows/ and
# plugins/linux/ and validates the declaration itself at load time.
$pluginArtifacts = @(
    @{ File = 'meow_plugin_prefetch.dll'; Platform = 'windows' }
)

$targetDir = Join-Path $repoRoot 'target\plugins-src'
$env:CARGO_TARGET_DIR = $targetDir
$build = "cd /d $repoRoot && cargo build --release --manifest-path plugins-src\Cargo.toml"
if (Test-Path $vcvars) {
    $build = "call `"$vcvars`" >nul 2>&1 && $build"
} elseif (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    Write-Error 'Visual Studio 2022 vcvars64.bat not found and link.exe is not on PATH; run this script from a VS developer environment (see CLAUDE.md).'
}
cmd.exe /d /s /c $build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:CARGO_TARGET_DIR = ''

$releaseDir = Join-Path $targetDir 'release'
$destinations = @(Join-Path $repoRoot 'target\release')
$tauriRelease = Join-Path $repoRoot 'apps\desktop\src-tauri\target\release'
if (Test-Path $tauriRelease) {
    $destinations += $tauriRelease
}

foreach ($artifact in $pluginArtifacts) {
    $source = Join-Path $releaseDir $artifact.File
    if (-not (Test-Path $source)) {
        Write-Error "plugin build output missing: $source"
    }
    foreach ($destination in $destinations) {
        $pluginDir = Join-Path $destination "plugins\$($artifact.Platform)"
        New-Item -ItemType Directory -Force -Path $pluginDir | Out-Null
        Copy-Item -Force $source (Join-Path $pluginDir $artifact.File)
        Write-Host "staged $($artifact.File) -> $pluginDir"
    }
}
