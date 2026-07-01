# Requires -Version 7
<#
.SYNOPSIS
  CI guard: detect field-name drift between transport DTOs and their
  TypeScript counterparts.
.DESCRIPTION
  crates/transport/src/dto/*.rs is the manual IPC contract (see CLAUDE.md
  "Gotchas" #1); there is no codegen, so Rust `*Dto` struct fields and their
  TypeScript interface fields must be kept in sync by hand. This script pairs
  each Rust `struct FooDto { ... }` with a TypeScript `interface Foo { ... }`
  (case.rs's naming convention: TS interfaces drop the `Dto` suffix — see
  CLAUDE.md "Naming conventions") and reports any field-name mismatch.

  Pairing is done purely by name (`FooDto` <-> `Foo`), field names are derived
  from `#[serde(rename = "...")]` when present, otherwise from
  `#[serde(rename_all = "camelCase")]` snake_case -> camelCase conversion.
  Rust structs with no `Foo` interface (kept under a different TS name, or a
  request/audit-only type with no frontend consumer) are reported as
  "unpaired" for visibility but do NOT fail the build — pairing by name is a
  heuristic, not a naming mandate, so an intentional rename is not a defect.
  Only a field mismatch *within a paired* Rust/TS type fails the build, since
  that's an actual contract break.
#>
param(
  [switch]$ListUnpaired
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$dtoDir = Join-Path $repoRoot 'crates/transport/src/dto'
$typesDir = Join-Path $repoRoot 'frontend/src/types'

function Convert-SnakeToCamel {
  param([string]$Value)
  $parts = $Value -split '_'
  $result = $parts[0]
  for ($i = 1; $i -lt $parts.Count; $i++) {
    if ($parts[$i].Length -gt 0) {
      $result += $parts[$i].Substring(0, 1).ToUpperInvariant() + $parts[$i].Substring(1)
    }
  }
  return $result
}

# ── Parse Rust DTO structs: name -> sorted field-name list ───────────
$rustStructs = @{}

Get-ChildItem -LiteralPath $dtoDir -Filter '*.rs' -File | ForEach-Object {
  $content = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8

  # Match `pub struct FooDto { ... }` bodies (non-greedy, single nesting level;
  # DTO structs in this codebase don't nest struct defs inside their bodies).
  $structMatches = [regex]::Matches($content, '(?s)pub\s+struct\s+(\w+Dto)\s*\{(.*?)\n\}')

  foreach ($m in $structMatches) {
    $structName = $m.Groups[1].Value
    $body = $m.Groups[2].Value

    # Skip structs using #[serde(flatten)] — their effective wire shape merges
    # another struct's fields in, which this line-oriented parser can't
    # resolve; excluded from comparison rather than falsely flagged as drift.
    if ($body -match '#\[serde\(flatten\)\]') {
      continue
    }

    $fields = New-Object System.Collections.Generic.List[string]
    $pendingRename = $null
    foreach ($line in ($body -split "`r?`n")) {
      if ($line -match '#\[serde\([^)]*rename\s*=\s*"([^"]+)"') {
        $pendingRename = $matches[1]
        continue
      }
      if ($line -match '^\s*pub\s+(\w+)\s*:') {
        $fieldName = $matches[1]
        if ($pendingRename) {
          $fields.Add($pendingRename)
        }
        else {
          $fields.Add((Convert-SnakeToCamel -Value $fieldName))
        }
        $pendingRename = $null
      }
    }

    if ($fields.Count -gt 0) {
      $rustStructs[$structName] = ($fields | Sort-Object -Unique)
    }
  }
}

# ── Parse TypeScript interfaces: name -> sorted field-name list ──────
$tsInterfaces = @{}

Get-ChildItem -LiteralPath $typesDir -Filter '*.ts' -File |
  Where-Object { $_.Name -notlike '*.test.ts' } |
  ForEach-Object {
    $content = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8

    $ifaceMatches = [regex]::Matches($content, '(?s)export\s+interface\s+(\w+)\s*\{(.*?)\n\}')
    foreach ($m in $ifaceMatches) {
      $ifaceName = $m.Groups[1].Value
      $body = $m.Groups[2].Value

      $fields = New-Object System.Collections.Generic.List[string]
      foreach ($line in ($body -split "`r?`n")) {
        if ($line -match '^\s*(\w+)\??\s*:') {
          $fields.Add($matches[1])
        }
      }

      if ($fields.Count -gt 0) {
        $tsInterfaces[$ifaceName] = ($fields | Sort-Object -Unique)
      }
    }
  }

# ── Pair by name (FooDto <-> Foo) and compare fields ──────────────────
$mismatches = @()
$paired = 0
$unpairedRust = @()

foreach ($structName in $rustStructs.Keys | Sort-Object) {
  $tsName = $structName -replace 'Dto$', ''
  if (-not $tsInterfaces.ContainsKey($tsName)) {
    $unpairedRust += $structName
    continue
  }

  $paired++
  $rustFields = $rustStructs[$structName]
  $tsFields = $tsInterfaces[$tsName]

  $onlyInRust = @(Compare-Object $rustFields $tsFields | Where-Object { $_.SideIndicator -eq '<=' } | ForEach-Object { $_.InputObject })
  $onlyInTs = @(Compare-Object $rustFields $tsFields | Where-Object { $_.SideIndicator -eq '=>' } | ForEach-Object { $_.InputObject })

  if ($onlyInRust.Count -gt 0 -or $onlyInTs.Count -gt 0) {
    $mismatches += [PSCustomObject]@{
      Rust        = $structName
      Ts          = $tsName
      OnlyInRust  = $onlyInRust
      OnlyInTs    = $onlyInTs
    }
  }
}

if ($ListUnpaired -and $unpairedRust.Count -gt 0) {
  Write-Host "INFO: Rust DTOs with no name-matching TS interface (not a failure - may use a different TS name):" -ForegroundColor Yellow
  $unpairedRust | ForEach-Object { Write-Host "  $_" }
}

if ($mismatches.Count -gt 0) {
  Write-Host "ERROR: DTO field drift detected between paired Rust/TypeScript types:" -ForegroundColor Red
  foreach ($mm in $mismatches) {
    Write-Host "  $($mm.Rust) <-> $($mm.Ts)"
    if ($mm.OnlyInRust.Count -gt 0) {
      Write-Host "    only in Rust: $($mm.OnlyInRust -join ', ')"
    }
    if ($mm.OnlyInTs.Count -gt 0) {
      Write-Host "    only in TS:   $($mm.OnlyInTs -join ', ')"
    }
  }
  exit 1
}

Write-Host "OK: DTO drift guard passed ($paired paired types checked, $($unpairedRust.Count) unpaired Rust DTOs)."
