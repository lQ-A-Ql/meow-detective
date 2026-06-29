# Requires -Version 7
<#
.SYNOPSIS
  CI guard: fail if production Rust source files contain #[allow(dead_code)].
.DESCRIPTION
  Dead-code allowances are acceptable inside integration tests, unit tests,
  benches, examples, and vendored third-party crates. This script scans the
  workspace source tree and fails when any production .rs file still carries
  an explicit #[allow(dead_code)] attribute.
#>
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$offenders = @()

$excludePatterns = @(
    '*/tests/*'
    '*/benches/*'
    '*/examples/*'
    '*/evtx-patched/*'
    '*/target/*'
    '*/.claude/*'
    '*/.git/*'
)

Get-ChildItem -Path $repoRoot -Filter '*.rs' -Recurse -File |
    Where-Object {
        $path = $_.FullName -replace '\\', '/'
        foreach ($pattern in $excludePatterns) {
            if ($path -like $pattern) { return $false }
        }
        return $true
    } |
    ForEach-Object {
        $matches = Select-String -Path $_.FullName -Pattern '#\[allow\s*\(\s*dead_code\s*\)\]' -ErrorAction SilentlyContinue
        if ($matches) {
            $offenders += $_.FullName.Substring($repoRoot.Length + 1)
        }
    }

if ($offenders.Count -gt 0) {
    Write-Host "ERROR: #[allow(dead_code)] found in production source files:" -ForegroundColor Red
    $offenders | Sort-Object | ForEach-Object { Write-Host "  $_" }
    exit 1
}

Write-Host "OK: no #[allow(dead_code)] attributes in production source files."
