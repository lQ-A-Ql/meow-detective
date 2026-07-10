# Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$archiveRoot = Join-Path $repoRoot 'docs/archive'
$manifestPath = Join-Path $archiveRoot 'manifest.json'
$progressPath = Join-Path $repoRoot 'docs/progress-ledger.md'

if (-not (Test-Path -LiteralPath $manifestPath)) {
  throw 'Archive manifest is missing: docs/archive/manifest.json'
}
if (-not (Test-Path -LiteralPath $progressPath)) {
  throw 'Project progress ledger is missing: docs/progress-ledger.md'
}

$strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
$docsFiles = Get-ChildItem -LiteralPath (Join-Path $repoRoot 'docs') -Recurse -File
foreach ($file in $docsFiles) {
  try {
    $null = $strictUtf8.GetString([System.IO.File]::ReadAllBytes($file.FullName))
  } catch {
    throw "Documentation file is not valid UTF-8: $($file.FullName)"
  }
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
$archiveMetadata = @(
  (Join-Path $archiveRoot 'README.md'),
  $manifestPath,
  (Join-Path $archiveRoot 'path-map.md')
)
$businessFiles = @(
  Get-ChildItem -LiteralPath $archiveRoot -Recurse -File |
    Where-Object { $_.FullName -notin $archiveMetadata }
)
if ($businessFiles.Count -ne $manifest.documentCount) {
  throw "Archive count mismatch: manifest=$($manifest.documentCount), actual=$($businessFiles.Count)"
}

foreach ($metadataPath in @($manifest.authorityIndex, $manifest.progressLedger, $manifest.pathMap)) {
  if ([string]::IsNullOrWhiteSpace($metadataPath)) {
    throw 'Archive manifest contains an empty routing path'
  }
  if (-not (Test-Path -LiteralPath (Join-Path $repoRoot $metadataPath))) {
    throw "Archive routing target is missing: $metadataPath"
  }
}

$pathMapPath = Join-Path $repoRoot $manifest.pathMap
$pathMapContent = Get-Content -LiteralPath $pathMapPath -Raw -Encoding UTF8
$pathPattern = '(?m)^\| `(?<old>docs/[^`]+)` \| `(?<new>docs/archive/[^`]+)` \|$'
$pathMappings = [regex]::Matches($pathMapContent, $pathPattern)
if ($pathMappings.Count -ne $manifest.documentCount) {
  throw "Archive path-map count mismatch: manifest=$($manifest.documentCount), actual=$($pathMappings.Count)"
}

$oldPaths = @{}
$newPaths = @{}
foreach ($mapping in $pathMappings) {
  $oldPath = $mapping.Groups['old'].Value
  $newPath = $mapping.Groups['new'].Value
  if ($oldPaths.ContainsKey($oldPath)) {
    throw "Archive path-map contains duplicate old path: $oldPath"
  }
  if ($newPaths.ContainsKey($newPath)) {
    throw "Archive path-map contains duplicate destination: $newPath"
  }
  $oldPaths[$oldPath] = $true
  $newPaths[$newPath] = $true
  if (Test-Path -LiteralPath (Join-Path $repoRoot $oldPath)) {
    throw "Archived document still exists at its old path: $oldPath"
  }
  if (-not (Test-Path -LiteralPath (Join-Path $repoRoot $newPath))) {
    throw "Archive path-map destination is missing: $newPath"
  }
}

$groupTotal = 0
foreach ($group in $manifest.groups) {
  if ($group.path -notmatch '^[a-z-]+/\d{4}-\d{2}$') {
    throw "Invalid archive group path: $($group.path)"
  }
  $groupPath = Join-Path $archiveRoot ($group.path -replace '/', [System.IO.Path]::DirectorySeparatorChar)
  if (-not (Test-Path -LiteralPath $groupPath)) {
    throw "Archive group directory is missing: $($group.path)"
  }
  $actual = @(Get-ChildItem -LiteralPath $groupPath -Recurse -File).Count
  if ($actual -ne $group.count) {
    throw "Archive group count mismatch for $($group.path): manifest=$($group.count), actual=$actual"
  }
  $groupTotal += $actual
}
if ($groupTotal -ne $manifest.documentCount) {
  throw "Archive group total mismatch: groups=$groupTotal, manifest=$($manifest.documentCount)"
}

$allowedRootAuditDocs = @('engineering-audit-plan.md')
$rootHistorical = @(
  Get-ChildItem -LiteralPath (Join-Path $repoRoot 'docs') -File |
    Where-Object {
      $_.Name -notin $allowedRootAuditDocs -and (
        $_.Name -like '*-audit-*.md' -or
        $_.Name -like '*-review*.md' -or
        $_.Name -like '*development-log*.md' -or
        $_.Name -like 'remediation-plan-*.md'
      )
    }
)
if ($rootHistorical.Count -gt 0) {
  throw "Historical documents must be archived by type/month: $($rootHistorical.Name -join ', ')"
}

Write-Host "Documentation archive guard passed ($($businessFiles.Count) archived documents)"
