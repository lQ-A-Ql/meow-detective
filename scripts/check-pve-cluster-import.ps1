param(
    [string]$FixtureRoot = $env:FORENSICS_PVE_CLUSTER_ROOT,
    [switch]$RequireFixture
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($FixtureRoot) -or -not (Test-Path -LiteralPath $FixtureRoot -PathType Container)) {
    $message = "FORENSICS_PVE_CLUSTER_ROOT is not set to a readable PVE cluster fixture directory."
    if ($RequireFixture) {
        throw $message
    }
    Write-Host "SKIP: $message"
    exit 0
}

$env:FORENSICS_PVE_CLUSTER_ROOT = (Resolve-Path -LiteralPath $FixtureRoot).Path

cargo test -p forensics-desktop --lib `
    real_pve_cluster_import_attempts_every_member_and_isolates_source_databases `
    -- --ignored --test-threads=1
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
