#Requires -Version 5.1

param(
    [string]$FixtureRoot = $env:FORENSICS_PVE_CLUSTER_ROOT,
    [switch]$RequireFixture,
    [ValidateRange(1, 86400)]
    [int]$TimeoutSeconds = 3600
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$expectedMembers = [string[]]@(
    "server01/server01-disk01.E01",
    "server01/server01-disk02.E01",
    "server02/server02-disk01.E01",
    "server02/server02-disk02.E01",
    "server03/server03-disk01.E01",
    "server03/server03-disk02.E01"
)
$testName = "commands::import::background_job::tests::real_pve_cluster_import_attempts_every_member_and_isolates_source_databases"

function Test-PrimaryClusterImage {
    param([Parameter(Mandatory = $true)][System.IO.FileInfo]$File)

    $extension = $File.Extension.TrimStart(".").ToLowerInvariant()
    if ($extension -match "^e\d{2}$" -and $extension -ne "e01") {
        return $false
    }
    return $extension -in @("e01", "ewf", "raw", "dd", "img")
}

function Get-FixtureRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $normalizedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd([char[]]@("\", "/"))
    $normalizedPath = [System.IO.Path]::GetFullPath($Path)
    $prefix = $normalizedRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $normalizedPath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "PVE fixture member is outside the fixture root: $normalizedPath"
    }
    return $normalizedPath.Substring($prefix.Length).Replace("\", "/")
}

if ([string]::IsNullOrWhiteSpace($FixtureRoot) -or
    -not (Test-Path -LiteralPath $FixtureRoot -PathType Container)) {
    $message = "FORENSICS_PVE_CLUSTER_ROOT is not set to a readable PVE cluster fixture directory."
    if ($RequireFixture) {
        throw $message
    }
    Write-Host "SKIP: $message"
    exit 0
}

$resolvedFixtureRoot = (Resolve-Path -LiteralPath $FixtureRoot).Path
$fixtureMembers = [string[]]@(
    Get-ChildItem -LiteralPath $resolvedFixtureRoot -Recurse -File -ErrorAction Stop |
        Where-Object { Test-PrimaryClusterImage -File $_ } |
        ForEach-Object {
            if ($_.Length -le 0) {
                throw "PVE fixture member is empty: $($_.FullName)"
            }
            Get-FixtureRelativePath -Root $resolvedFixtureRoot -Path $_.FullName
        }
)
[System.Array]::Sort($fixtureMembers, [System.StringComparer]::OrdinalIgnoreCase)

$fixtureMatches = $fixtureMembers.Count -eq $expectedMembers.Count
if ($fixtureMatches) {
    for ($index = 0; $index -lt $expectedMembers.Count; $index++) {
        if ($fixtureMembers[$index] -cne $expectedMembers[$index]) {
            $fixtureMatches = $false
            break
        }
    }
}
if (-not $fixtureMatches) {
    throw @"
PVE fixture preflight failed. The planner-visible primary image set must be exactly:
$($expectedMembers -join [Environment]::NewLine)
Actual:
$($fixtureMembers -join [Environment]::NewLine)
"@
}

$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$env:FORENSICS_PVE_CLUSTER_ROOT = $resolvedFixtureRoot
. (Join-Path $PSScriptRoot "lib/RustGuard.Common.ps1")

$cargo = Get-Command cargo -CommandType Application -ErrorAction Stop | Select-Object -First 1
$startInfo = New-Object System.Diagnostics.ProcessStartInfo
$startInfo.FileName = $cargo.Source
$startInfo.Arguments = "test -p forensics-desktop --lib $testName -- --ignored --exact --nocapture --test-threads=1"
$startInfo.WorkingDirectory = $projectRoot
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true

Write-Host "PVE fixture preflight passed in exact import order:"
$expectedMembers | ForEach-Object { Write-Host "  $_" }
Write-Host "Running exact serial regression with timeout ${TimeoutSeconds}s: cargo $($startInfo.Arguments)"

$result = Invoke-RustGuardProcess `
    -StartInfo $startInfo `
    -TimeoutMilliseconds ($TimeoutSeconds * 1000) `
    -TimeoutContext "real six-member PVE cluster import regression"
if (-not [string]::IsNullOrEmpty($result.Stdout)) {
    [Console]::Out.Write($result.Stdout)
}
if (-not [string]::IsNullOrEmpty($result.Stderr)) {
    [Console]::Error.Write($result.Stderr)
}
if ($result.ExitCode -ne 0) {
    exit $result.ExitCode
}
