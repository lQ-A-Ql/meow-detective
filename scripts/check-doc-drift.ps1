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

function Get-KnownLimitationDocRows {
  param([Parameter(Mandatory = $true)][string]$Content)

  $lines = $Content -split "\r?\n"
  $sectionStart = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^## 2\.') {
      $sectionStart = $i
      break
    }
  }

  if ($sectionStart -lt 0) {
    throw 'known-unsupported-formats.md section 2 heading not found'
  }

  $rows = New-Object System.Collections.Generic.List[string]
  $tableStarted = $false
  for ($i = $sectionStart + 1; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -match '^## 3\.') {
      break
    }
    if ($line -match '^## ' -and $line -notmatch '^## 2\.') {
      break
    }
    if (-not $tableStarted) {
      if ($line -match '^\|') {
        $tableStarted = $true
      }
      continue
    }
    if ($line -match '^\|---\|---\|---\|---\|$') {
      continue
    }
    if ($line -match '^\| .+ \| .+ \| .+ \| .+ \|$') {
      $rows.Add($line)
      continue
    }
    if (-not [string]::IsNullOrWhiteSpace($line)) {
      break
    }
  }

  if ($rows.Count -eq 0) {
    throw 'known-unsupported-formats.md section 2 table rows not found'
  }

  return $rows
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
$knownUnsupportedDoc = Read-Text 'docs/known-unsupported-formats.md'
$releaseScorecardDoc = Read-Text 'docs/release-scorecard.md'
$validationDoc = Read-Text 'docs/validation-trust-framework.md'
$progressLedger = Read-Text 'docs/progress-ledger.md'
$stage7Acceptance = Read-Text 'docs/backend-stage7-final-acceptance.md'
$knownLimitationsFact = Read-Text 'testdata/governance/v2-known-limitations.json' | ConvertFrom-Json
$benchmarkBaseline = Read-Text 'testdata/governance/v2-benchmark-baseline.json' | ConvertFrom-Json

$workspaceManifest = Read-Text 'Cargo.toml'
$workspaceMembersMatch = [regex]::Match(
  $workspaceManifest,
  '(?ms)^\[workspace\]\s*.*?^\s*members\s*=\s*\[(?<members>.*?)^\s*\]'
)
if (-not $workspaceMembersMatch.Success) {
  throw 'Cargo.toml workspace.members could not be parsed'
}
$crateCount = ([regex]::Matches(
    $workspaceMembersMatch.Groups['members'].Value,
    '"crates/[^"\r\n]+"'
  ) | Measure-Object).Count
Assert-Equals $crateCount 35 'Workspace crate count drifted'
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
$knownLimitationRows = Get-KnownLimitationDocRows $knownUnsupportedDoc
$moduleBaselineRows = @(Import-Csv -LiteralPath (Join-Path $repoRoot 'scripts/baselines/rust-module-size-baseline.csv'))
$functionBaselineRows = @(Import-Csv -LiteralPath (Join-Path $repoRoot 'scripts/baselines/rust-function-size-baseline.csv'))
$moduleDebtCount = $moduleBaselineRows.Count
$functionDebtCount = $functionBaselineRows.Count
$hardFunctionDebtCount = @(
  $functionBaselineRows | Where-Object { [int]$_.lines -gt 150 }
).Count
$appServicesModuleDebtCount = @(
  $moduleBaselineRows | Where-Object { $_.path -like 'crates/app-services/*' }
).Count
$appServicesFunctionDebtCount = @(
  $functionBaselineRows | Where-Object { $_.path -like 'crates/app-services/*' }
).Count

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
Assert-Equals $appServicesModuleDebtCount 0 'app-services module baseline debt was reintroduced'
Assert-Equals $appServicesFunctionDebtCount 0 'app-services function baseline debt was reintroduced'
Assert-Matches -Content $progressLedger -Pattern "baseline\s+$moduleDebtCount[^\r\n]+baseline\s+$functionDebtCount[^\r\n]+$hardFunctionDebtCount[^\r\n]+150" -Message 'progress-ledger structural debt facts are stale'
Assert-Contains -Content $stage7Acceptance -Needle "| Module-size baseline rows | 83 | $moduleDebtCount |" -Message 'Stage 7 module baseline result is stale'
Assert-Contains -Content $stage7Acceptance -Needle "| Function-size baseline rows | 65 | $functionDebtCount |" -Message 'Stage 7 function baseline result is stale'
Assert-Contains -Content $stage7Acceptance -Needle "| Historic functions above 150 lines | not separately closed | $hardFunctionDebtCount |" -Message 'Stage 7 hard-function debt result is stale'

Assert-Equals $mermaidCount 15 'Mermaid diagram count drifted'
Assert-Equals $knownLimitationsFact.documentedLimitCount $knownLimitationsFact.items.Count 'known limitations documentedLimitCount drifted'
Assert-Equals $knownLimitationRows.Count $knownLimitationsFact.documentedLimitCount 'known unsupported formats table row count drifted'
Assert-Contains $docIndex 'testdata/governance/v2-known-limitations.json' 'documentation-index is missing known limitations fact source'
Assert-Contains $releaseScorecardDoc 'testdata/governance/v2-known-limitations.json' 'release-scorecard is missing known limitations fact source'
Assert-Contains $validationDoc 'testdata/governance/v2-known-limitations.json' 'validation-trust-framework is missing known limitations fact source'

foreach ($item in $knownLimitationsFact.items) {
  Assert-Contains $knownUnsupportedDoc $item.item "known-unsupported-formats.md is missing item: $($item.item)"
  Assert-Contains $knownUnsupportedDoc $item.summary "known-unsupported-formats.md is missing summary: $($item.summary)"
}

# ── Benchmark baseline structural validation ─────────────────────
Assert-Contains $benchmarkBaseline.hostProfile 'Windows' 'benchmark-baseline hostProfile is missing Windows'
Assert-Equals $benchmarkBaseline.baselineVersion '2026.06' 'benchmark-baseline baselineVersion drifted'
Assert-Contains $benchmarkBaseline.lastVerifiedAt '2026-06-13' 'benchmark-baseline lastVerifiedAt is stale'
$requiredCheckCount = $benchmarkBaseline.requiredChecks.Count
$scenarioCount = $benchmarkBaseline.scenarios.Count
Assert-Contains $readme "v2-benchmark-baseline.json" 'README is missing benchmark baseline fact source'
Assert-Contains $docIndex "v2-benchmark-baseline.json" 'documentation-index is missing benchmark baseline fact source'
Assert-Contains $agents "v2-benchmark-baseline.json" 'AGENTS is missing benchmark baseline fact source'

# Verify required checks reference existing scenarios
$scenarioKeys = @{}
foreach ($s in $benchmarkBaseline.scenarios) {
  $key = "$($s.datasetLevel)/$($s.scenario)"
  $scenarioKeys[$key] = $s
}
foreach ($check in $benchmarkBaseline.requiredChecks) {
  $key = "$($check.datasetLevel)/$($check.scenario)"
  if (-not $scenarioKeys.ContainsKey($key)) {
    throw "benchmark-baseline requiredCheck references nonexistent scenario: $key"
  }
}

# Verify all six scenarios exist for each dataset level
$requiredLevels = @('small','medium','large')
$requiredScenarios = @('search_query','file_tree_expand','file_paginate','timeline_filter','artifact_extract','report_export')
foreach ($level in $requiredLevels) {
  foreach ($scenario in $requiredScenarios) {
    $key = "$level/$scenario"
    if (-not $scenarioKeys.ContainsKey($key)) {
      throw "benchmark-baseline is missing scenario: $key"
    }
  }
}
foreach ($level in $requiredLevels) {
  foreach ($scenario in $requiredScenarios) {
    $key = "$level/$scenario"
    $check = $benchmarkBaseline.requiredChecks | Where-Object {
      "$($_.datasetLevel)/$($_.scenario)" -eq $key
    } | Select-Object -First 1
    if ($null -eq $check) {
      throw "benchmark-baseline is missing required check: $key"
    }
  }
}

foreach ($path in @(
    'docs/engineering-audit-plan.md',
    'docs/development-engineering-guide.md',
    'docs/design-constraints.md',
    'docs/model-architecture-algorithm-diagrams.md',
    'docs/documentation-index.md',
    'docs/v2-longterm-plan.md',
    'docs/fixture-handbook.md',
    'docs/expected-json-contract.md',
    'docs/error-classification-manual.md',
    'docs/benchmark-baseline.md',
    'docs/correlation-analysis-design.md',
    'docs/release-scorecard.md'
  )) {
  if (-not (Test-Path -LiteralPath (Join-Path $repoRoot $path))) {
    throw "Required engineering document is missing: $path"
  }
  Assert-Contains $readme $path "README is missing engineering doc entry: $path"
  Assert-Contains $agents $path "AGENTS is missing engineering doc entry: $path"
  Assert-Contains $docIndex $path "documentation-index is missing engineering doc entry: $path"
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
