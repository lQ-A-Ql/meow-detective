# Requires -Version 7
<#
.SYNOPSIS
  CI guard: flag oversized production modules before they grow into
  unmaintainable monoliths.
.DESCRIPTION
  Scans crates/**/*.rs (excluding tests/benches/examples/vendored code and
  target/build output) and fails if any production Rust module exceeds 1500
  lines. Also scans frontend/src/**/*.ts(x) (excluding *.test.ts(x)) and warns
  (without failing) if any production component/hook file exceeds 500 lines,
  per the module-size guidance in CLAUDE.md (Rust <=500 lines, frontend
  <=300 lines as a *target*; this guard uses more permissive hard limits so
  it only catches genuine monoliths, not every file above the target).
#>
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

$RUST_MAX_LINES = 1500
$FRONTEND_WARN_LINES = 500

$rustExcludePatterns = @(
    '*/tests/*'
    '*/benches/*'
    '*/examples/*'
    '*/evtx-patched/*'
    '*/target/*'
    '*/.claude/*'
    '*/.git/*'
)

$frontendExcludePatterns = @(
    '*/node_modules/*'
    '*/dist/*'
    '*/.claude/*'
    '*/.git/*'
)

function Get-LineCount {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-Content -LiteralPath $Path -ErrorAction SilentlyContinue | Measure-Object -Line).Lines
}

# ── Rust: hard failure above RUST_MAX_LINES ──────────────────────────
$rustOffenders = @()

Get-ChildItem -Path (Join-Path $repoRoot 'crates') -Filter '*.rs' -Recurse -File |
    Where-Object {
        $path = $_.FullName -replace '\\', '/'
        foreach ($pattern in $rustExcludePatterns) {
            if ($path -like $pattern) { return $false }
        }
        return $true
    } |
    ForEach-Object {
        $lines = Get-LineCount -Path $_.FullName
        if ($lines -gt $RUST_MAX_LINES) {
            $rustOffenders += [PSCustomObject]@{
                Path  = $_.FullName.Substring($repoRoot.Length + 1)
                Lines = $lines
            }
        }
    }

if ($rustOffenders.Count -gt 0) {
    Write-Host "ERROR: production Rust modules exceed $RUST_MAX_LINES lines:" -ForegroundColor Red
    $rustOffenders | Sort-Object -Property Lines -Descending | ForEach-Object {
        Write-Host ("  {0} ({1} lines)" -f $_.Path, $_.Lines)
    }
    exit 1
}

Write-Host "OK: no Rust module in crates/ exceeds $RUST_MAX_LINES lines."

# ── Frontend: soft warning above FRONTEND_WARN_LINES ─────────────────
$frontendOffenders = @()

Get-ChildItem -Path (Join-Path $repoRoot 'frontend/src') -Include '*.ts', '*.tsx' -Recurse -File |
    Where-Object {
        $path = $_.FullName -replace '\\', '/'
        if ($_.Name -like '*.test.ts' -or $_.Name -like '*.test.tsx' -or $_.Name -like '*.spec.ts' -or $_.Name -like '*.spec.tsx') {
            return $false
        }
        foreach ($pattern in $frontendExcludePatterns) {
            if ($path -like $pattern) { return $false }
        }
        return $true
    } |
    ForEach-Object {
        $lines = Get-LineCount -Path $_.FullName
        if ($lines -gt $FRONTEND_WARN_LINES) {
            $frontendOffenders += [PSCustomObject]@{
                Path  = $_.FullName.Substring($repoRoot.Length + 1)
                Lines = $lines
            }
        }
    }

if ($frontendOffenders.Count -gt 0) {
    Write-Host "WARNING: frontend files exceed $FRONTEND_WARN_LINES lines (consider splitting):" -ForegroundColor Yellow
    $frontendOffenders | Sort-Object -Property Lines -Descending | ForEach-Object {
        Write-Host ("  {0} ({1} lines)" -f $_.Path, $_.Lines)
    }
} else {
    Write-Host "OK: no frontend file in frontend/src exceeds $FRONTEND_WARN_LINES lines."
}

Write-Host "Module size guard passed"
