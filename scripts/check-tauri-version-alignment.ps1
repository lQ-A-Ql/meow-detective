$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$lock = Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.lock') -Raw -Encoding UTF8
$pnpmLock = Get-Content -LiteralPath (Join-Path $repoRoot 'frontend/pnpm-lock.yaml') -Raw -Encoding UTF8
$packages = [regex]::Matches($lock, '(?ms)^\[\[package\]\]\s*\r?\n(.*?)(?=^\[\[package\]\]|\z)')

foreach ($pair in @(
    @{ Rust = 'tauri'; Npm = '@tauri-apps/api' },
    @{ Rust = 'tauri-plugin-dialog'; Npm = '@tauri-apps/plugin-dialog' }
)) {
    $namePattern = '(?m)^name\s*=\s*"{0}"\s*$' -f [regex]::Escape($pair.Rust)
    $matched = @($packages | Where-Object { $_.Groups[1].Value -match $namePattern })
    if ($matched.Count -ne 1) {
        throw "Expected one resolved $($pair.Rust) package in Cargo.lock"
    }
    $version = [regex]::Match($matched[0].Groups[1].Value, '(?m)^version\s*=\s*"([^"]+)"\s*$')
    $rustVersion = $version.Groups[1].Value
    $npmPath = Join-Path $repoRoot ("frontend/node_modules/{0}/package.json" -f $pair.Npm)
    if (-not (Test-Path -LiteralPath $npmPath -PathType Leaf)) {
        throw "Install frontend dependencies with pnpm --dir frontend install --frozen-lockfile before checking Tauri versions"
    }
    $npmVersion = (Get-Content -LiteralPath $npmPath -Raw -Encoding UTF8 | ConvertFrom-Json).version
    $rustRelease = [regex]::Match($rustVersion, '^(\d+\.\d+)\.\d+(?:[-+].*)?$')
    $npmRelease = [regex]::Match($npmVersion, '^(\d+\.\d+)\.\d+(?:[-+].*)?$')
    if (-not $rustRelease.Success -or -not $npmRelease.Success -or
        $rustRelease.Groups[1].Value -ne $npmRelease.Groups[1].Value) {
        throw "Mismatched Tauri packages: $($pair.Rust) $rustVersion / $($pair.Npm) $npmVersion. Major/minor releases must match."
    }
    $resolvedPattern = '(?m)^\s*''{0}@(\d+\.\d+)\.\d+[^'']*'':' -f [regex]::Escape($pair.Npm)
    $resolved = [regex]::Matches($pnpmLock, $resolvedPattern)
    if ($resolved.Count -eq 0) { throw "No resolved $($pair.Npm) package found in the pnpm lockfile" }
    foreach ($entry in $resolved) {
        if ($entry.Groups[1].Value -ne $rustRelease.Groups[1].Value) {
            throw "Transitive $($pair.Npm) $($entry.Groups[1].Value).x does not match $($pair.Rust) $rustVersion"
        }
    }
    Write-Host "$($pair.Rust) $rustVersion / $($pair.Npm) $npmVersion : aligned"
}
