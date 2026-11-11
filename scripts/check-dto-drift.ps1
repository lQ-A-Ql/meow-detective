# Requires -Version 7
<#
.SYNOPSIS
  CI guard: detect contract drift between transport DTOs and their
  TypeScript counterparts.
.DESCRIPTION
  crates/transport/src/dto/**/*.rs is the manual IPC contract (see CLAUDE.md
  "Gotchas" #1); there is no codegen, so Rust `*Dto` struct fields and their
  TypeScript definitions must be kept in sync by hand. This script pairs each
  Rust `struct FooDto { ... }` with a TypeScript `interface Foo` or object
  `type Foo = { ... }` (case.rs's naming convention: TS types drop the `Dto`
  suffix — see CLAUDE.md "Naming conventions") and reports contract drift.

  Pairing is done purely by name (`FooDto` <-> `Foo`), field names are derived
  from `#[serde(rename = "...")]` when present, otherwise from the struct's
  `#[serde(rename_all = "...")]` mode (camelCase default). Comparison is
  case-sensitive: silent acronym-casing drift (`appcompatLayers` vs
  `appCompatLayers`) is a real wire break, so ordinal semantics are required.

  For paired types the guard checks:
    - field-name sets (missing/extra fields),
    - optionality: a Rust `Option<T>` must surface as an optional or nullable
      TS field, and a Rust field that is neither optional, defaulted, nor
      conditionally skipped must not be optional in TS.

  TypeScript `extends` chains are resolved so inherited fields are compared
  against the Rust struct. Rust structs with no same-name TS type (kept under a
  different TS name, or a request/audit-only type with no frontend consumer)
  are reported as "unpaired" for visibility but do NOT fail the build — pairing
  by name is a heuristic, not a naming mandate, so an intentional rename is not
  a defect. Only a mismatch *within a paired* Rust/TS type fails the build,
  since that's an actual contract break.
#>
param(
  [switch]$ListUnpaired
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$dtoDir = Join-Path $repoRoot 'crates/transport/src/dto'
$typesDir = Join-Path $repoRoot 'frontend/src/types'

function Convert-SnakeToCase {
  param(
    [string]$Value,
    [string]$RenameAll = 'camelCase'
  )
  $parts = @($Value -split '_' | Where-Object { $_.Length -gt 0 })
  if ($parts.Count -eq 0) { return $Value }
  switch ($RenameAll) {
    'snake_case' { return ($parts -join '_') }
    'SCREAMING_SNAKE_CASE' { return ($parts -join '_').ToUpperInvariant() }
    'PascalCase' { return (($parts | ForEach-Object { $_.Substring(0, 1).ToUpperInvariant() + $_.Substring(1) }) -join '') }
    'kebab-case' { return ($parts -join '-').ToLowerInvariant() }
    'SCREAMING-KEBAB-CASE' { return ($parts -join '-').ToUpperInvariant() }
    default {
      $result = $parts[0]
      for ($i = 1; $i -lt $parts.Count; $i++) {
        $result += $parts[$i].Substring(0, 1).ToUpperInvariant() + $parts[$i].Substring(1)
      }
      return $result
    }
  }
}

function New-OrdinalMap {
  return [System.Collections.Generic.Dictionary[string, object]]::new([System.StringComparer]::Ordinal)
}

# ── Parse Rust DTO structs ────────────────────────────────────────────
# name -> @{ Names = sorted string[]; Fields = ordinal map name -> meta }
$rustStructs = @{}

Get-ChildItem -LiteralPath $dtoDir -Filter '*.rs' -File -Recurse | ForEach-Object {
  $content = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8

  # Capture the attribute block in front of the struct so its rename_all mode
  # is honoured; the body is matched non-greedily through the closing brace
  # line (DTO structs in this codebase don't nest struct definitions).
  $structMatches = [regex]::Matches(
    $content,
    '(?s)((?:#\[[^\]]*\]\s*)*)pub\s+struct\s+(\w+Dto)\s*\{(.*?)\n\}'
  )

  foreach ($m in $structMatches) {
    $structAttrs = $m.Groups[1].Value
    $structName = $m.Groups[2].Value
    $body = $m.Groups[3].Value

    # Skip structs using #[serde(flatten)] — their effective wire shape merges
    # another struct's fields in, which this line-oriented parser can't
    # resolve; excluded from comparison rather than falsely flagged as drift.
    if ($body -match '#\[serde\(flatten\)\]') {
      continue
    }

    $renameAll = 'camelCase'
    $renameAllMatch = [regex]::Match($structAttrs, 'rename_all\s*=\s*"([^"]+)"')
    if ($renameAllMatch.Success) {
      $renameAll = $renameAllMatch.Groups[1].Value
    }

    $fieldNames = New-Object System.Collections.Generic.List[string]
    $fieldMeta = New-OrdinalMap
    $pendingAttributes = New-Object System.Collections.Generic.List[string]

    foreach ($line in ($body -split "`r?`n")) {
      $trimmed = $line.Trim()
      if ($trimmed.StartsWith('#[')) {
        $pendingAttributes.Add($trimmed)
        continue
      }
      if ($line -match '^\s*pub\s+(\w+)\s*:\s*(.+?),?\s*$') {
        $fieldName = $matches[1]
        $fieldType = $matches[2].Trim()
        $attributeText = $pendingAttributes -join ' '
        $pendingAttributes.Clear()

        # #[serde(skip)] removes the field from the wire shape entirely; it is
        # not contract surface, so neither side should be compared on it.
        if ($attributeText -match '#\[serde\([^)]*\bskip\s*[,)]') {
          continue
        }

        $renameMatch = [regex]::Match($attributeText, 'rename\s*=\s*"([^"]+)"')
        $wireName = if ($renameMatch.Success) {
          $renameMatch.Groups[1].Value
        } else {
          Convert-SnakeToCase -Value $fieldName -RenameAll $renameAll
        }

        $fieldNames.Add($wireName)
        $fieldMeta[$wireName] = [PSCustomObject]@{
          Name        = $wireName
          IsOption    = $fieldType.StartsWith('Option<')
          HasDefault  = [bool]($attributeText -match '#\[serde\([^)]*\bdefault\b')
          HasSkipIf   = $attributeText -match 'skip_serializing_if'
        }
      } else {
        $pendingAttributes.Clear()
      }
    }

    if ($fieldNames.Count -gt 0) {
      $rustStructs[$structName] = @{
        Names  = @($fieldNames | Sort-Object -Unique -CaseSensitive)
        Fields = $fieldMeta
      }
    }
  }
}

# ── Parse TypeScript interfaces and object type aliases ──────────────
# name -> @{ Fields = ordinal map; Parents = string[] }, then resolve extends.
$tsDeclarations = @{}

function Read-TsDeclarationBody {
  param(
    [string]$Content,
    [int]$BodyStart
  )
  $depth = 1
  $cursor = $BodyStart
  while ($cursor -lt $Content.Length -and $depth -gt 0) {
    $ch = $Content[$cursor]
    if ($ch -eq '{') { $depth++ } elseif ($ch -eq '}') { $depth-- }
    $cursor++
  }
  $bodyLength = [Math]::Max(0, $cursor - $BodyStart - 1)
  return $Content.Substring($BodyStart, $bodyLength)
}

Get-ChildItem -LiteralPath $typesDir -Filter '*.ts' -File -Recurse |
  Where-Object { $_.Name -notlike '*.test.ts' } |
  ForEach-Object {
    $content = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8

    # Brace-counted so a one-line declaration no longer swallows the following
    # declarations into its captured body. `extends A, B` is captured so
    # inherited fields can be resolved before comparing against the DTO.
    $declMatches = [regex]::Matches(
      $content,
      'export\s+(interface|type)\s+(\w+)\s*(?:extends\s+([^{=]+?))?\s*(?:=\s*)?\{'
    )
    foreach ($m in $declMatches) {
      $kind = $m.Groups[1].Value
      if ($kind -eq 'type' -and $m.Value -notmatch '=\s*\{') {
        continue
      }

      $declName = $m.Groups[2].Value
      $parents = @()
      if ($m.Groups[3].Success) {
        $parents = @($m.Groups[3].Value -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_.Length -gt 0 })
      }

      $body = Read-TsDeclarationBody -Content $content -BodyStart ($m.Index + $m.Length)
      # Comments may contain "word:" prose (e.g. "Log origin: ..."), which would
      # be read as a phantom field, so strip them before scanning.
      $body = [regex]::Replace($body, '/\*[\s\S]*?\*/|//[^\n]*', '')

      $fields = New-OrdinalMap
      # Scan the whole body so one-line declarations contribute every field.
      foreach ($fm in [regex]::Matches($body, '(?:^|[\s;{])(\w+)(\?)?\s*:\s*([^;}\n]+)')) {
        $fields[$fm.Groups[1].Value] = [PSCustomObject]@{
          Optional = $fm.Groups[2].Success
          Type     = $fm.Groups[3].Value.Trim()
        }
      }

      $tsDeclarations[$declName] = @{
        Fields  = $fields
        Parents = $parents
      }
    }
  }

$tsFieldCache = @{}
function Resolve-TsFields {
  param(
    [string]$Name,
    [System.Collections.Generic.HashSet[string]]$Visiting
  )
  if ($tsFieldCache.ContainsKey($Name)) { return $tsFieldCache[$Name] }
  if (-not $tsDeclarations.ContainsKey($Name)) { return $null }
  if (-not $Visiting.Add($Name)) { return $null }

  $declaration = $tsDeclarations[$Name]
  $merged = New-OrdinalMap
  foreach ($parent in $declaration.Parents) {
    $parentFields = Resolve-TsFields -Name $parent -Visiting $Visiting
    if ($null -ne $parentFields) {
      foreach ($entry in $parentFields.GetEnumerator()) {
        $merged[$entry.Key] = $entry.Value
      }
    }
  }
  foreach ($entry in $declaration.Fields.GetEnumerator()) {
    $merged[$entry.Key] = $entry.Value
  }

  [void]$Visiting.Remove($Name)
  $tsFieldCache[$Name] = $merged
  return $merged
}

# ── Pair by name (FooDto <-> Foo) and compare ─────────────────────────
$mismatches = @()
$paired = 0
$unpairedRust = @()

foreach ($structName in $rustStructs.Keys | Sort-Object) {
  $tsName = $structName -replace 'Dto$', ''
  $tsFields = Resolve-TsFields -Name $tsName -Visiting ([System.Collections.Generic.HashSet[string]]::new())
  if ($null -eq $tsFields) {
    $unpairedRust += $structName
    continue
  }

  $paired++
  $rustFieldNames = $rustStructs[$structName].Names
  $tsFieldNames = @($tsFields.Keys | Sort-Object -Unique -CaseSensitive)

  # Ordinal (case-sensitive) comparison: camelCase acronym casing is contract.
  $onlyInRust = @(Compare-Object -CaseSensitive $rustFieldNames $tsFieldNames |
      Where-Object { $_.SideIndicator -eq '<=' } |
      ForEach-Object { $_.InputObject })
  $onlyInTs = @(Compare-Object -CaseSensitive $rustFieldNames $tsFieldNames |
      Where-Object { $_.SideIndicator -eq '=>' } |
      ForEach-Object { $_.InputObject })

  $optionality = @()
  foreach ($rustFieldName in $rustFieldNames) {
    if (-not $tsFields.ContainsKey($rustFieldName)) { continue }
    $rustField = $rustStructs[$structName].Fields[$rustFieldName]
    $tsField = $tsFields[$rustFieldName]
    $tsNullable = $tsField.Type -match '\b(null|undefined)\b'

    if ($rustField.IsOption -and -not $tsField.Optional -and -not $tsNullable) {
      $optionality += "$rustFieldName (Rust Option, TS requires it)"
      continue
    }
    if (-not $rustField.IsOption -and -not $rustField.HasSkipIf -and -not $rustField.HasDefault -and $tsField.Optional) {
      $optionality += "$rustFieldName (Rust always serializes it, TS marks it optional)"
    }
    if (-not $rustField.IsOption -and -not $rustField.HasSkipIf -and $tsNullable) {
      $optionality += "$rustFieldName (Rust never serializes null, TS allows null)"
    }
  }

  if ($onlyInRust.Count -gt 0 -or $onlyInTs.Count -gt 0 -or $optionality.Count -gt 0) {
    $mismatches += [PSCustomObject]@{
      Rust        = $structName
      Ts          = $tsName
      OnlyInRust  = $onlyInRust
      OnlyInTs    = $onlyInTs
      Optionality = $optionality
    }
  }
}

if ($ListUnpaired -and $unpairedRust.Count -gt 0) {
  Write-Host "INFO: Rust DTOs with no same-name TS type (not a failure - may use a different TS name):" -ForegroundColor Yellow
  $unpairedRust | ForEach-Object { Write-Host "  $_" }
}

if ($mismatches.Count -gt 0) {
  Write-Host "ERROR: DTO contract drift detected between paired Rust/TypeScript types:" -ForegroundColor Red
  foreach ($mm in $mismatches) {
    Write-Host "  $($mm.Rust) <-> $($mm.Ts)"
    if ($mm.OnlyInRust.Count -gt 0) {
      Write-Host "    only in Rust: $($mm.OnlyInRust -join ', ')"
    }
    if ($mm.OnlyInTs.Count -gt 0) {
      Write-Host "    only in TS:   $($mm.OnlyInTs -join ', ')"
    }
    if ($mm.Optionality.Count -gt 0) {
      foreach ($entry in $mm.Optionality) {
        Write-Host "    optionality:  $entry"
      }
    }
  }
  exit 1
}

Write-Host "OK: DTO drift guard passed ($paired paired types checked, $($unpairedRust.Count) unpaired Rust DTOs)."
