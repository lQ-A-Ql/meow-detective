param(
  [switch]$Json,
  [switch]$AdvisoriesOnly,
  [switch]$BansOnly,
  [switch]$LicensesOnly,
  [switch]$SourcesOnly,
  [switch]$AllowDirty,
  [int]$TimeoutSeconds = 300
)

$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$denyPath = Join-Path $projectRoot "deny.toml"
if (-not (Test-Path -LiteralPath $denyPath)) {
  throw "deny.toml not found at $denyPath"
}

$cargoDeny = Get-Command cargo-deny -ErrorAction SilentlyContinue
if (-not $cargoDeny) {
  Write-Error "cargo-deny is not installed. Install with: cargo install cargo-deny"
  exit 1
}

Push-Location $projectRoot
try {
  if ($AllowDirty) {
    $env:CARGO_DENY_ALLOW_DIRTY = "1"
  }

  $all = -not ($AdvisoriesOnly -or $BansOnly -or $LicensesOnly -or $SourcesOnly)

  $results = [ordered]@{
    timestamp    = (Get-Date -Format "o")
    project      = "forensics-workbench"
    denyConfig   = (Resolve-Path $denyPath -Relative)
    advisories   = $null
    bans         = $null
    licenses     = $null
    sources      = $null
    summary      = [ordered]@{ pass = $true; violations = @() }
  }

  # -- advisories -------------------------------------------------------
  if ($all -or $AdvisoriesOnly) {
    $advisoryResult = [ordered]@{ status = "ok"; diagnostics = @(); error = $null }
    try {
      $output = & cargo deny check advisories 2>&1
      $exitCode = $LASTEXITCODE
      $outputText = ($output | Out-String).Trim()

      $advisoryResult.status = if ($exitCode -eq 0) { "ok" } else { "violations-found" }
      $advisoryResult.diagnostics = $outputText -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }

      if ($exitCode -ne 0) {
        $results.summary.pass = $false
        $results.summary.violations += "advisories"
      }
    } catch {
      $advisoryResult.status = "error"
      $advisoryResult.error = $_.Exception.Message
      $results.summary.pass = $false
      $results.summary.violations += "advisories:error"
    }
    $results.advisories = $advisoryResult
  }

  # -- bans -------------------------------------------------------------
  if ($all -or $BansOnly) {
    $banResult = [ordered]@{ status = "ok"; diagnostics = @(); error = $null }
    try {
      $output = & cargo deny check bans 2>&1
      $exitCode = $LASTEXITCODE
      $outputText = ($output | Out-String).Trim()

      $banResult.status = if ($exitCode -eq 0) { "ok" } else { "violations-found" }
      $banResult.diagnostics = $outputText -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }

      if ($exitCode -ne 0) {
        $results.summary.pass = $false
        $results.summary.violations += "bans"
      }
    } catch {
      $banResult.status = "error"
      $banResult.error = $_.Exception.Message
      $results.summary.pass = $false
      $results.summary.violations += "bans:error"
    }
    $results.bans = $banResult
  }

  # -- licenses ---------------------------------------------------------
  if ($all -or $LicensesOnly) {
    $licenseResult = [ordered]@{ status = "ok"; diagnostics = @(); error = $null }
    try {
      $output = & cargo deny check licenses 2>&1
      $exitCode = $LASTEXITCODE
      $outputText = ($output | Out-String).Trim()

      $licenseResult.status = if ($exitCode -eq 0) { "ok" } else { "violations-found" }
      $licenseResult.diagnostics = $outputText -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }

      if ($exitCode -ne 0) {
        $results.summary.pass = $false
        $results.summary.violations += "licenses"
      }
    } catch {
      $licenseResult.status = "error"
      $licenseResult.error = $_.Exception.Message
      $results.summary.pass = $false
      $results.summary.violations += "licenses:error"
    }
    $results.licenses = $licenseResult
  }

  # -- sources ----------------------------------------------------------
  if ($all -or $SourcesOnly) {
    $sourceResult = [ordered]@{ status = "ok"; diagnostics = @(); error = $null }
    try {
      $output = & cargo deny check sources 2>&1
      $exitCode = $LASTEXITCODE
      $outputText = ($output | Out-String).Trim()

      $sourceResult.status = if ($exitCode -eq 0) { "ok" } else { "violations-found" }
      $sourceResult.diagnostics = $outputText -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }

      if ($exitCode -ne 0) {
        $results.summary.pass = $false
        $results.summary.violations += "sources"
      }
    } catch {
      $sourceResult.status = "error"
      $sourceResult.error = $_.Exception.Message
      $results.summary.pass = $false
      $results.summary.violations += "sources:error"
    }
    $results.sources = $sourceResult
  }

  # Emit results
  if ($Json) {
    $results | ConvertTo-Json -Depth 6 -Compress
  } else {
    Write-Host "=== Dependency Security Check ===" -ForegroundColor Cyan
    Write-Host "Project : $($results.project)"
    Write-Host "Config  : $($results.denyConfig)"
    Write-Host "Timestamp: $($results.timestamp)"
    Write-Host ""

    $checks = @(
      @{ Label = "Advisories"; Result = $results.advisories },
      @{ Label = "Bans";       Result = $results.bans },
      @{ Label = "Licenses";   Result = $results.licenses },
      @{ Label = "Sources";    Result = $results.sources }
    )

    foreach ($check in $checks) {
      if ($null -eq $check.Result) { continue }
      $color = if ($check.Result.status -eq "ok") { "Green" } else { "Red" }
      Write-Host "[$($check.Label)] $($check.Result.status)" -ForegroundColor $color
      if ($check.Result.error) {
        Write-Host "  Error: $($check.Result.error)" -ForegroundColor Red
      }
      foreach ($line in $check.Result.diagnostics) {
        Write-Host "  $line"
      }
      Write-Host ""
    }

    if ($results.summary.pass) {
      Write-Host "Result: ALL CHECKS PASSED" -ForegroundColor Green
      exit 0
    } else {
      $violations = $results.summary.violations -join ", "
      Write-Host "Result: VIOLATIONS FOUND ($violations)" -ForegroundColor Red
      exit 1
    }
  }
} finally {
  Pop-Location
}
