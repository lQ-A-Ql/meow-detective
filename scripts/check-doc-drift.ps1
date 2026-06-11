param(
  [switch]$RenderMermaid
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location -LiteralPath $repoRoot

function Read-Text {
  param([Parameter(Mandatory = $true)][string]$Path)
  return Get-Content -LiteralPath (Join-Path $repoRoot $Path) -Raw -Encoding UTF8
}

function Assert-Contains {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Needle,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if (-not $Content.Contains($Needle)) {
    throw $Message
  }
}

function Assert-Equals {
  param(
    [Parameter(Mandatory = $true)]$Actual,
    [Parameter(Mandatory = $true)]$Expected,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Actual -ne $Expected) {
    throw "$Message Actual=$Actual Expected=$Expected"
  }
}

function Assert-Matches {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Content -notmatch $Pattern) {
    throw $Message
  }
}

function Assert-TableFact {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$StableMarker,
    [Parameter(Mandatory = $true)]$ExpectedValue,
    [Parameter(Mandatory = $true)][string]$Message
  )

  $escapedMarker = [regex]::Escape($StableMarker)
  $pattern = "\|[^\r\n|]*\|[^\r\n|]*$ExpectedValue[^\r\n|]*\|[^\r\n|]*$escapedMarker[^\r\n|]*\|"
  if ($Content -notmatch $pattern) {
    throw $Message
  }
}

$readme = Read-Text 'README.md'
$agents = Read-Text 'AGENTS.md'
$docIndex = Read-Text 'docs/documentation-index.md'
$diagramDoc = Read-Text 'docs/model-architecture-algorithm-diagrams.md'

$crateCount = (Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates') -Directory | Measure-Object).Count
$commandFiles = Get-ChildItem -LiteralPath (Join-Path $repoRoot 'apps/desktop/src-tauri/src/commands') -Recurse -File -Filter '*.rs'
$commandCount = ($commandFiles | Select-String -Pattern '#\[tauri::command\]' | Measure-Object).Count
$repoCount = (Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates/persistence-sqlite/src/repositories') -Filter '*_repo.rs' | Measure-Object).Count
$migrationCount = (Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates/persistence-sqlite/src/migrations/scripts') -Filter '*.sql' | Measure-Object).Count
$pageCount = (Get-ChildItem -LiteralPath (Join-Path $repoRoot 'frontend/src/app/pages') -Filter '*.tsx' | Where-Object { $_.Name -notlike '*.test.tsx' } | Measure-Object).Count
$frontendTestCount = (
  Get-ChildItem -LiteralPath (Join-Path $repoRoot 'frontend/src') -Recurse -File |
    Where-Object { $_.Name -like '*.test.ts' -or $_.Name -like '*.test.tsx' } |
    Measure-Object
).Count
$serviceModuleCount = (Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates/app-services/src') -Filter '*.rs' | Where-Object { $_.Name -ne 'lib.rs' } | Measure-Object).Count
$mermaidCount = ([regex]::Matches($diagramDoc, '```mermaid')).Count

Assert-Contains $readme "$crateCount Rust crates" 'README crate count is stale'
Assert-Contains $readme "$pageCount frontend pages" 'README frontend page count is stale'
Assert-Contains $readme "$commandCount Tauri commands" 'README Tauri command count is stale'
Assert-Contains $readme "$serviceModuleCount source modules" 'README app-services module count is stale'
Assert-Contains $readme "migration scripts ($migrationCount)" 'README migration script count is stale'
Assert-Contains $readme "$frontendTestCount test files" 'README frontend test file count is stale'

Assert-Contains $agents "$repoCount repos, $migrationCount migration scripts" 'AGENTS persistence-sqlite count is stale'
Assert-Contains $agents "$frontendTestCount frontend test files" 'AGENTS frontend test file count is stale'

Assert-Contains $docIndex "Rust workspace crate | $crateCount" 'documentation-index crate count is stale'
Assert-Contains $docIndex "Tauri commands | $commandCount" 'documentation-index command count is stale'
Assert-Contains $docIndex "app-services source modules | $serviceModuleCount" 'documentation-index app-services module count is stale'
Assert-Contains $docIndex "SQLite repositories | $repoCount" 'documentation-index repository count is stale'
Assert-Contains $docIndex "SQLite migration scripts | $migrationCount" 'documentation-index migration count is stale'
Assert-TableFact $docIndex 'frontend/src/app/pages/*.tsx' $pageCount 'documentation-index frontend page row is stale'
Assert-TableFact $docIndex 'frontend/src/**/*.test.ts(x)' $frontendTestCount 'documentation-index frontend test row is stale'
Assert-Matches $docIndex "\|\s*[^|]*Mermaid[^|]*\|\s*$mermaidCount\s*\|" 'documentation-index Mermaid count is stale'

Assert-Equals $mermaidCount 14 'Mermaid diagram count drifted'

foreach ($path in @(
    'docs/engineering-audit-plan.md',
    'docs/development-engineering-guide.md',
    'docs/design-constraints.md',
    'docs/model-architecture-algorithm-diagrams.md',
    'docs/documentation-index.md'
  )) {
  if (-not (Test-Path -LiteralPath (Join-Path $repoRoot $path))) {
    throw "Required engineering document is missing: $path"
  }
  Assert-Contains $readme $path "README is missing engineering doc entry: $path"
  Assert-Contains $agents $path "AGENTS is missing engineering doc entry: $path"
}

if ($RenderMermaid) {
  $edge = @(
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
  ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1

  if (-not $edge) {
    throw 'Mermaid render requested, but Chrome/Edge executable was not found'
  }

  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('forensics-mermaid-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $tempRoot | Out-Null
  try {
    $configPath = Join-Path $tempRoot 'puppeteer-config.json'
    $escapedEdge = $edge.Replace('\', '\\')
    $json = '{"executablePath":"' + $escapedEdge + '","args":["--no-sandbox","--disable-setuid-sandbox"]}'
    [System.IO.File]::WriteAllText($configPath, $json, [System.Text.UTF8Encoding]::new($false))

    $matches = [regex]::Matches($diagramDoc, '```mermaid\r?\n([\s\S]*?)\r?\n```')
    for ($i = 0; $i -lt $matches.Count; $i++) {
      $mmdPath = Join-Path $tempRoot ('diagram-{0:D2}.mmd' -f ($i + 1))
      [System.IO.File]::WriteAllText($mmdPath, $matches[$i].Groups[1].Value, [System.Text.UTF8Encoding]::new($false))
    }

    $mmdFiles = Get-ChildItem -LiteralPath $tempRoot -Filter '*.mmd'
    foreach ($file in $mmdFiles) {
      $svgPath = [System.IO.Path]::ChangeExtension($file.FullName, '.svg')
      npx --yes @mermaid-js/mermaid-cli@11.4.2 -p $configPath -i $file.FullName -o $svgPath --quiet
      if (-not (Test-Path -LiteralPath $svgPath)) {
        throw "Mermaid render did not create SVG for $($file.Name)"
      }
    }

    $svgCount = (Get-ChildItem -LiteralPath $tempRoot -Filter '*.svg' | Measure-Object).Count
    Assert-Equals $svgCount $mmdFiles.Count 'Mermaid rendered SVG count mismatch'
  }
  finally {
    if (Test-Path -LiteralPath $tempRoot) {
      Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
  }
}

Write-Host "Documentation drift guard passed"
