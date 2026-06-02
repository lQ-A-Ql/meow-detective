param(
  [switch]$Rust,
  [switch]$Frontend,
  [switch]$StrictRustTool
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

if (-not $Rust -and -not $Frontend) {
  $Rust = $true
  $Frontend = $true
}

$coverageRoot = Join-Path $repoRoot 'coverage'
New-Item -ItemType Directory -Force -Path $coverageRoot | Out-Null

if ($Rust) {
  $llvmCov = Get-Command cargo-llvm-cov -ErrorAction SilentlyContinue
  if ($llvmCov) {
    Write-Host 'Running Rust coverage with cargo-llvm-cov...'
    cargo llvm-cov --workspace --all-targets --lcov --output-path (Join-Path $coverageRoot 'rust-lcov.info')
  } elseif ($StrictRustTool) {
    throw 'cargo-llvm-cov is required for Rust coverage. Install it with: cargo install cargo-llvm-cov --locked'
  } else {
    Write-Warning 'Skipping Rust coverage: cargo-llvm-cov is not installed. Install with: cargo install cargo-llvm-cov --locked'
  }
}

if ($Frontend) {
  Write-Host 'Running frontend coverage with Vitest...'
  pnpm --dir frontend test:coverage

  $frontendSummary = Join-Path $repoRoot 'frontend\coverage\coverage-summary.json'
  if (-not (Test-Path -LiteralPath $frontendSummary)) {
    throw "Expected frontend coverage summary was not generated: $frontendSummary"
  }
}

Write-Host "Coverage reports are available under $coverageRoot and frontend\coverage."
