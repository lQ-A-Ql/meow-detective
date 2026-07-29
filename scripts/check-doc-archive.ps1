# Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$gitIgnorePath = Join-Path $repoRoot '.gitignore'
$gitIgnore = Get-Content -LiteralPath $gitIgnorePath -Raw -Encoding UTF8

if (-not $gitIgnore.Contains('/docs/**')) {
  throw 'Documentation retention policy is missing the /docs/** default-ignore rule'
}

$trackedIgnored = @(
  & git -C $repoRoot ls-files -ci --exclude-standard -- docs
)
if ($LASTEXITCODE -ne 0) {
  throw 'Unable to query tracked documentation against .gitignore'
}
if ($trackedIgnored.Count -gt 0) {
  throw "Development-process documents remain tracked despite the retention policy: $($trackedIgnored -join ', ')"
}

$trackedDocs = @(& git -C $repoRoot ls-files -- docs)
if ($LASTEXITCODE -ne 0) {
  throw 'Unable to enumerate tracked documentation'
}
if ($trackedDocs.Count -eq 0) {
  throw 'Documentation retention policy left no tracked technical documents'
}

$strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
foreach ($relativePath in $trackedDocs) {
  $path = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Tracked documentation is missing from the worktree: $relativePath"
  }
  try {
    $null = $strictUtf8.GetString([System.IO.File]::ReadAllBytes($path))
  } catch {
    throw "Tracked documentation is not valid UTF-8: $relativePath"
  }
}

$requiredTechnicalDocs = @(
  'docs/architecture-model.md',
  'docs/backend-module-architecture.md',
  'docs/design-constraints.md',
  'docs/documentation-index.md',
  'docs/model-architecture-algorithm-diagrams.md',
  'docs/parser-support-matrix.md',
  'docs/validation-trust-framework.md'
)
foreach ($relativePath in $requiredTechnicalDocs) {
  if ($relativePath -notin $trackedDocs) {
    throw "Required technical document is not tracked: $relativePath"
  }
}

Write-Host "Documentation retention guard passed ($($trackedDocs.Count) durable technical documents)"
