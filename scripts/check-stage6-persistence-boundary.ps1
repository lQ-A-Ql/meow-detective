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

function Get-WorkspaceDependencyManifests {
  $manifests = New-Object System.Collections.Generic.List[string]
  foreach ($root in @('crates', 'apps')) {
    $absoluteRoot = Join-Path $repoRoot $root
    if (-not (Test-Path -LiteralPath $absoluteRoot -PathType Container)) {
      continue
    }
    foreach ($owner in Get-ChildItem -LiteralPath $absoluteRoot -Directory) {
      $candidates = @(Join-Path $owner.FullName 'Cargo.toml')
      foreach ($child in Get-ChildItem -LiteralPath $owner.FullName -Directory) {
        $candidates += Join-Path $child.FullName 'Cargo.toml'
      }
      foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
          $manifests.Add($candidate)
        }
      }
    }
  }
  return $manifests
}

function Assert-SingleFjallDependencyOwner {
  $pattern = '(?m)^\s*persistence-fjall(\s*\.\w+\s*)?='
  $offenders = New-Object System.Collections.Generic.List[string]
  foreach ($manifest in Get-WorkspaceDependencyManifests) {
    $relative = $manifest.Substring($repoRoot.Length + 1).Replace('\', '/')
    if ($relative -ieq 'crates/persistence-source/Cargo.toml') {
      continue
    }
    if ((Read-StrictUtf8 $manifest) -match $pattern) {
      $offenders.Add($relative)
    }
  }
  if ($offenders.Count -gt 0) {
    $errors.Add('only crates/persistence-source/Cargo.toml may declare a persistence-fjall dependency; offenders: ' + ($offenders -join ', '))
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
Assert-ManifestLacksDependency 'crates/app-services/Cargo.toml' 'persistence-fjall'
Assert-ManifestLacksDependency 'crates/analysis-core/Cargo.toml' 'persistence-fjall'
Assert-ManifestLacksDependency 'crates/emulation-services/Cargo.toml' 'persistence-fjall'
Assert-ManifestLacksDependency 'crates/import-core/Cargo.toml' 'persistence-fjall'
Assert-ManifestLacksDependency 'crates/source-reconstruction/Cargo.toml' 'persistence-fjall'
Assert-SingleFjallDependencyOwner

Assert-NoRustReference 'crates/persistence-sqlite/src' '\bpersistence_fjall\b' 'persistence-sqlite owns source detail storage'
Assert-NoRustReference 'crates/persistence-sqlite/tests' '\bpersistence_fjall\b' 'persistence-sqlite tests own source detail storage'
Assert-NoRustReference 'crates/app-services/src' '\bpersistence_fjall\b' 'app-services must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/app-services/tests' '\bpersistence_fjall\b' 'app-services tests must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/analysis-core/src' '\bpersistence_fjall\b' 'analysis-core must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/analysis-core/tests' '\bpersistence_fjall\b' 'analysis-core tests must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/emulation-services/src' '\bpersistence_fjall\b' 'emulation-services must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/emulation-services/tests' '\bpersistence_fjall\b' 'emulation-services tests must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/import-core/src' '\bpersistence_fjall\b' 'import-core must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/import-core/tests' '\bpersistence_fjall\b' 'import-core tests must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/source-reconstruction/src' '\bpersistence_fjall\b' 'source-reconstruction must consume the persistence-source fjall facade'
Assert-NoRustReference 'crates/source-reconstruction/tests' '\bpersistence_fjall\b' 'source-reconstruction tests must consume the persistence-source fjall facade'
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

Write-Host 'Stage 6 persistence boundary guard passed: persistence-sqlite is case-store only, Fjall source storage is owned by persistence-source, and app-services consumes only the persistence-source fjall facade'
