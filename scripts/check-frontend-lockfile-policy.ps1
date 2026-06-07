$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$frontendRoot = Join-Path $repoRoot "frontend"
$pnpmLock = Join-Path $frontendRoot "pnpm-lock.yaml"
$npmLock = Join-Path $frontendRoot "package-lock.json"
$packageJson = Join-Path $frontendRoot "package.json"

if (-not (Test-Path -LiteralPath $packageJson)) {
  throw "Frontend package.json is missing: $packageJson"
}

if (-not (Test-Path -LiteralPath $pnpmLock)) {
  throw "Frontend pnpm lockfile is missing: $pnpmLock"
}

if (Test-Path -LiteralPath $npmLock) {
  throw "Frontend uses pnpm; remove stale npm lockfile: $npmLock"
}

$package = Get-Content -LiteralPath $packageJson -Raw -Encoding UTF8 | ConvertFrom-Json
if (-not $package.packageManager -or -not ($package.packageManager -like "pnpm@*")) {
  throw "frontend/package.json must declare a pinned pnpm packageManager"
}

Write-Host "Frontend lockfile policy guard passed"
