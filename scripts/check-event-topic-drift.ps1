param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$rustPath = Join-Path $repoRoot "crates/transport/src/events/mod.rs"
$tsPath = Join-Path $repoRoot "frontend/src/types/events.ts"

foreach ($path in @($rustPath, $tsPath)) {
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required event contract file is missing: $path"
  }
}

$rust = Get-Content -LiteralPath $rustPath -Raw -Encoding UTF8
$ts = Get-Content -LiteralPath $tsPath -Raw -Encoding UTF8

function Convert-PascalToKebab {
  param([string]$Value)
  $withHyphens = [regex]::Replace($Value, '(?<!^)(?=[A-Z])', '-')
  return $withHyphens.ToLowerInvariant()
}

# Wire strings declared as TOPIC_* constants. These are the canonical source for
# the IPC payload topic field.
$rustConstTopics = [regex]::Matches($rust, 'pub\s+const\s+TOPIC_[A-Z_]+:\s*&str\s*=\s*"([^"]+)";') |
  ForEach-Object { $_.Groups[1].Value } |
  Sort-Object -Unique

# Parse the EventTopic enum block and derive the wire string for each variant,
# respecting #[serde(rename = "...")].
$enumMatch = [regex]::Match($rust, '(?s)pub\s+enum\s+EventTopic\s*\{(.*?)\}')
if (-not $enumMatch.Success) {
  throw "Could not locate EventTopic enum in $rustPath"
}
$enumBlock = $enumMatch.Groups[1].Value

$rustEnumTopics = @()
$pendingRename = $null
foreach ($line in ($enumBlock -split "`r?`n")) {
  if ($line -match '#\[\s*serde\s*\(\s*rename\s*=\s*"([^"]+)"\s*\)\s*\]') {
    $pendingRename = $matches[1]
  }
  elseif ($line -match '^\s*([A-Za-z][A-Za-z0-9_]*)\s*,?\s*(?://.*)?$') {
    $variant = $matches[1]
    if ($null -ne $pendingRename) {
      $wire = $pendingRename
    }
    else {
      $wire = Convert-PascalToKebab -Value $variant
    }
    $rustEnumTopics += $wire
    $pendingRename = $null
  }
}
$rustEnumTopics = $rustEnumTopics | Sort-Object -Unique

# TypeScript union literal strings.
$tsMatch = [regex]::Match($ts, '(?s)type\s+EventTopic\s*=\s*([^;]+);')
if (-not $tsMatch.Success) {
  throw "Could not locate EventTopic type in $tsPath"
}
$tsBlock = $tsMatch.Groups[1].Value
$tsTopics = [regex]::Matches($tsBlock, "'([^']+)'") |
  ForEach-Object { $_.Groups[1].Value } |
  Sort-Object -Unique

$onlyInRustConsts = @(Compare-Object $rustConstTopics $tsTopics |
  Where-Object { $_.SideIndicator -eq '<=' } |
  ForEach-Object { $_.InputObject })
$onlyInTsFromConsts = @(Compare-Object $rustConstTopics $tsTopics |
  Where-Object { $_.SideIndicator -eq '=>' } |
  ForEach-Object { $_.InputObject })

$onlyInRustEnum = @(Compare-Object $rustEnumTopics $tsTopics |
  Where-Object { $_.SideIndicator -eq '<=' } |
  ForEach-Object { $_.InputObject })
$onlyInTsFromEnum = @(Compare-Object $rustEnumTopics $tsTopics |
  Where-Object { $_.SideIndicator -eq '=>' } |
  ForEach-Object { $_.InputObject })

$constDrift = ($onlyInRustConsts.Count -gt 0) -or ($onlyInTsFromConsts.Count -gt 0)
$enumDrift = ($onlyInRustEnum.Count -gt 0) -or ($onlyInTsFromEnum.Count -gt 0)

if ($constDrift -or $enumDrift) {
  $messages = @()
  if ($onlyInRustConsts) {
    $messages += "Rust TOPIC_* constants not in TypeScript union: $($onlyInRustConsts -join ', ')"
  }
  if ($onlyInTsFromConsts) {
    $messages += "TypeScript union topics not in Rust TOPIC_* constants: $($onlyInTsFromConsts -join ', ')"
  }
  if ($onlyInRustEnum) {
    $messages += "Rust EventTopic variants not in TypeScript union: $($onlyInRustEnum -join ', ')"
  }
  if ($onlyInTsFromEnum) {
    $messages += "TypeScript union topics not in Rust EventTopic variants: $($onlyInTsFromEnum -join ', ')"
  }
  throw ($messages -join "`n")
}

Write-Host "EventTopic drift guard passed: $($rustEnumTopics.Count) topics synchronized between Rust and TypeScript"
