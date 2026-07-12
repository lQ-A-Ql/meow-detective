param()

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$strictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)
$errors = New-Object System.Collections.Generic.List[string]

function Read-StrictUtf8([string]$Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -ge 3 -and
      $bytes[0] -eq 0xEF -and
      $bytes[1] -eq 0xBB -and
      $bytes[2] -eq 0xBF) {
    throw "UTF-8 BOM is not allowed: $Path"
  }
  return $strictUtf8.GetString($bytes)
}

function Count-Lines([string]$Content) {
  if ($Content.Length -eq 0) {
    return 0
  }
  return ($Content -split "`r?`n").Count
}

function Resolve-Facade([string[]]$Candidates, [string]$Capability) {
  $matches = New-Object System.Collections.Generic.List[string]
  foreach ($candidate in $Candidates) {
    $absolutePath = Join-Path $repoRoot $candidate
    if (Test-Path -LiteralPath $absolutePath -PathType Leaf) {
      $matches.Add($candidate)
    }
  }
  if ($matches.Count -ne 1) {
    $errors.Add(
      "$Capability must have exactly one facade; found $($matches.Count): " +
      ($Candidates -join ', ')
    )
    return $null
  }
  return $matches[0]
}

$facadeGroups = @(
  @{
    Capability = 'Btrfs filesystem'
    Candidates = @('crates/fs-btrfs/src/lib.rs')
  },
  @{
    Capability = 'ext4 filesystem'
    Candidates = @('crates/fs-ext4/src/lib.rs')
  },
  @{
    Capability = 'XFS filesystem'
    Candidates = @('crates/fs-xfs/src/lib.rs')
  },
  @{
    Capability = 'LVM volume mapping'
    Candidates = @('crates/fs-lvm/src/lib.rs')
  },
  @{
    Capability = 'EVTX artifact parser'
    Candidates = @(
      'crates/artifacts-windows/src/evtx/parser.rs',
      'crates/artifacts-windows/src/evtx/parser/mod.rs'
    )
  },
  @{
    Capability = 'Firefox artifact parser'
    Candidates = @(
      'crates/artifacts-windows/src/browser/firefox.rs',
      'crates/artifacts-windows/src/browser/firefox/mod.rs'
    )
  },
  @{
    Capability = 'Registry SAM structures'
    Candidates = @(
      'crates/artifacts-windows/src/registry/sam_structs.rs',
      'crates/artifacts-windows/src/registry/sam_structs/mod.rs'
    )
  },
  @{
    Capability = 'Registry SAM lookup'
    Candidates = @(
      'crates/artifacts-windows/src/registry/lookup/sam.rs',
      'crates/artifacts-windows/src/registry/lookup/sam/mod.rs'
    )
  },
  @{
    Capability = 'mbox container parser'
    Candidates = @(
      'crates/containers-pst/src/mbox.rs',
      'crates/containers-pst/src/mbox/mod.rs'
    )
  },
  @{
    Capability = 'GQL parser'
    Candidates = @(
      'crates/gql/src/parser.rs',
      'crates/gql/src/parser/mod.rs'
    )
  },
  @{
    Capability = 'Linux journal parser'
    Candidates = @(
      'crates/artifacts-linux/src/journal.rs',
      'crates/artifacts-linux/src/journal/mod.rs'
    )
  }
)

foreach ($group in $facadeGroups) {
  $relativePath = Resolve-Facade $group.Candidates $group.Capability
  if ($null -eq $relativePath) {
    continue
  }
  $content = Read-StrictUtf8 (Join-Path $repoRoot $relativePath)
  $lineCount = Count-Lines $content
  if ($lineCount -gt 200) {
    $errors.Add(
      "$($group.Capability) facade exceeds 200 lines: " +
      "$relativePath ($lineCount)"
    )
  }
}

$crateRoots = @(
  'crates/fs-btrfs/src',
  'crates/fs-ext4/src',
  'crates/fs-xfs/src',
  'crates/fs-lvm/src',
  'crates/artifacts-windows/src',
  'crates/artifacts-linux/src',
  'crates/containers-pst/src',
  'crates/gql/src'
)

$allSources = New-Object System.Collections.Generic.List[string]
foreach ($relativeRoot in $crateRoots) {
  $absoluteRoot = Join-Path $repoRoot $relativeRoot
  foreach ($file in Get-ChildItem -LiteralPath $absoluteRoot -Recurse -File -Filter '*.rs') {
    $allSources.Add((Read-StrictUtf8 $file.FullName))
  }
}
$joinedSources = $allSources -join "`n"

if ($joinedSources -cmatch '\btauri::|#\[tauri::command\]|\bAppHandle\b|\bEmitter\b') {
  $errors.Add('parser and core crates must remain independent from the Tauri runtime')
}

foreach ($requiredSymbol in @(
  'BtrfsReader',
  'Ext4Reader',
  'XfsReader',
  'LvmPool',
  'probe_lvm',
  'extract_boot_shutdown_events',
  'parse_firefox_history',
  'parse_mbox',
  'pub fn parse(',
  'parse_journal'
)) {
  if (-not $joinedSources.Contains($requiredSymbol)) {
    $errors.Add("Stage 5 public parser surface is missing: $requiredSymbol")
  }
}

if ($errors.Count -gt 0) {
  Write-Error "Stage 5 parser boundary guard failed:`n$($errors -join "`n")"
}

Write-Host 'Stage 5 parser boundary guard passed'
