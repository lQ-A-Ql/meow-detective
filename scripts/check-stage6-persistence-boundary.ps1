param()

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$strictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)
$errors = New-Object System.Collections.Generic.List[string]

function Read-StrictUtf8([string]$Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
    throw "UTF-8 BOM is not allowed: $Path"
  }
  return $strictUtf8.GetString($bytes)
}

function Assert-ManifestLacksDependency([string]$RelativePath, [string]$Dependency) {
  $absolutePath = Join-Path $repoRoot $RelativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    $errors.Add("missing manifest: $RelativePath")
    return
  }
  $pattern = '(?m)^\s*' + [regex]::Escape($Dependency) + '(\s*\.\w+\s*)?='
  if ((Read-StrictUtf8 $absolutePath) -match $pattern) {
    $errors.Add("$RelativePath must not declare a $Dependency dependency")
  }
}

function Assert-NoRustReference([string]$RelativeRoot, [string]$Pattern, [string]$Message) {
  $absoluteRoot = Join-Path $repoRoot $RelativeRoot
  if (-not (Test-Path -LiteralPath $absoluteRoot -PathType Container)) {
    return
  }
  foreach ($file in Get-ChildItem -LiteralPath $absoluteRoot -Recurse -File -Filter '*.rs') {
    if ((Read-StrictUtf8 $file.FullName) -match $Pattern) {
      $relative = $file.FullName.Substring($repoRoot.Length + 1).Replace('\', '/')
      $errors.Add("${Message}: $relative")
    }
  }
}

Assert-ManifestLacksDependency 'crates/persistence-sqlite/Cargo.toml' 'persistence-fjall'
Assert-ManifestLacksDependency 'crates/persistence-sqlite/Cargo.toml' 'transport'
Assert-ManifestLacksDependency 'apps/desktop/src-tauri/Cargo.toml' 'persistence-fjall'

Assert-NoRustReference 'crates/persistence-sqlite/src' '\bpersistence_fjall\b' 'persistence-sqlite owns source detail storage'
Assert-NoRustReference 'crates/persistence-sqlite/tests' '\bpersistence_fjall\b' 'persistence-sqlite tests own source detail storage'
Assert-NoRustReference 'crates/persistence-sqlite/src' '\btransport\s*::' 'persistence-sqlite depends on the transport contract'
Assert-NoRustReference 'crates/persistence-sqlite/tests' '\btransport\s*::' 'persistence-sqlite tests depend on the transport contract'

foreach ($relativePath in @(
  'crates/persistence-source/src/detail/mod.rs',
  'crates/persistence-source/src/publication/mod.rs',
  'crates/persistence-source/src/control/mod.rs'
)) {
  if (-not (Test-Path -LiteralPath (Join-Path $repoRoot $relativePath) -PathType Leaf)) {
    $errors.Add("persistence-source is missing owned source storage: $relativePath")
  }
}

if (Test-Path -LiteralPath (Join-Path $repoRoot 'crates/persistence-sqlite/src/source_fjall.rs') -PathType Leaf) {
  $errors.Add('persistence-sqlite must not resurrect the source_fjall module')
}

$sourceManifestPath = Join-Path $repoRoot 'crates/persistence-source/Cargo.toml'
if ((Read-StrictUtf8 $sourceManifestPath) -notmatch '(?m)^\s*persistence-fjall(\s*\.\w+\s*)?=') {
  $errors.Add('persistence-source must own the persistence-fjall dependency')
}

$sourceLib = Read-StrictUtf8 (Join-Path $repoRoot 'crates/persistence-source/src/lib.rs')
if (-not $sourceLib.Contains('pub use persistence_fjall as fjall;')) {
  $errors.Add('persistence-source must expose the persistence_fjall facade as `fjall`')
}

if ($errors.Count -gt 0) {
  throw ("Stage 6 persistence boundary guard failed:`n- " + ($errors -join "`n- "))
}

Write-Host 'Stage 6 persistence boundary guard passed: persistence-sqlite is case-store only and Fjall source storage is owned by persistence-source'
