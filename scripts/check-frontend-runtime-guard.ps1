param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$frontendSrc = Join-Path $repoRoot "frontend/src"
$apiClientPath = Join-Path $frontendSrc "lib/api/client.ts"

if (-not (Test-Path -LiteralPath $frontendSrc)) {
  throw "Frontend source root is missing: $frontendSrc"
}
if (-not (Test-Path -LiteralPath $apiClientPath)) {
  throw "Frontend API client is missing: $apiClientPath"
}

function Get-RelativePath {
  param([Parameter(Mandatory = $true)][string]$Path)
  return $Path.Substring($repoRoot.Path.Length + 1).Replace('\', '/')
}

function Is-TestOrFixtureFile {
  param([Parameter(Mandatory = $true)][System.IO.FileInfo]$File)
  $relative = Get-RelativePath $File.FullName
  return (
    $relative -match '(^|/)src/test/' -or
    $relative -match '\.(test|spec)\.(ts|tsx)$' -or
    $relative -match '(^|/)__tests__(/|$)' -or
    $relative -match '(^|/)fixtures?(/|$)'
  )
}

function Assert-NoMatchesInRuntimeFiles {
  param(
    [Parameter(Mandatory = $true)][System.IO.FileInfo[]]$Files,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$Message,
    [string[]]$AllowedRelativePaths = @()
  )

  $violations = @()
  foreach ($file in $Files) {
    $relative = Get-RelativePath $file.FullName
    if ($AllowedRelativePaths -contains $relative) {
      continue
    }

    $content = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8
    $matches = [regex]::Matches($content, $Pattern)
    foreach ($match in $matches) {
      $lineNumber = ($content.Substring(0, $match.Index) -split "`n").Count
      $line = ($content -split "`n")[$lineNumber - 1].Trim()
      $violations += "{0}:{1}: {2}" -f $relative, $lineNumber, $line
    }
  }

  if ($violations.Count -gt 0) {
    throw "$Message`n$($violations -join "`n")"
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

$runtimeFiles = @(Get-ChildItem -LiteralPath $frontendSrc -Recurse -File -Include *.ts,*.tsx |
  Where-Object { -not (Is-TestOrFixtureFile $_) })

$apiClient = Get-Content -LiteralPath $apiClientPath -Raw -Encoding UTF8
Assert-Matches `
  -Content $apiClient `
  -Pattern "import\s+\{\s*invoke\s*\}\s+from\s+['""]@tauri-apps/api/core['""]" `
  -Message "frontend/src/lib/api/client.ts must remain the single Tauri invoke entry point"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern "['""]@tauri-apps/api/core['""]|\binvoke\s*\(" `
  -AllowedRelativePaths @('frontend/src/lib/api/client.ts') `
  -Message "Frontend runtime code must not call Tauri invoke directly; route through frontend/src/lib/api/client.ts"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern '\bvi\.mock\b|\bjest\.mock\b|\bmockResolvedValue\b|\bmockRejectedValue\b' `
  -Message "Frontend runtime code must not contain test/mock API wiring"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern '\b(?:mock|fake|dummy)(?:Data|Rows|Items|Results|Response|Payload|Case|Source|Artifact|Event|Node|Edge)s?\b' `
  -Message "Frontend runtime code must not define mock/fake/dummy business datasets"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern 'mockMode|demoMode|useMock|enableMock|MOCK_|FAKE_|DUMMY_' `
  -Message "Frontend runtime code must not expose mock/demo runtime modes"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern 'create_analysis_demo_case|CREATE_ANALYSIS_DEMO_CASE|createAnalysisDemoCase|useCreateAnalysisDemoCase|demoPending|onLoadDemoCase|加载演示案件' `
  -Message "Frontend runtime code must not expose production demo-case creation entry points"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern "from\s+['""]@/lib/api/(?!client['""]|mcp['""])[^'""]+['""]" `
  -AllowedRelativePaths @(Get-ChildItem -LiteralPath (Join-Path $frontendSrc "features") -Recurse -File -Include *.ts,*.tsx |
    ForEach-Object { Get-RelativePath $_.FullName }) `
  -Message "Frontend pages/components/stores must not import business API modules directly; route through feature hooks"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern 'Object\.keys\s*\(\s*[^)]*nodeCountByType[^)]*\)|getNodeNeighborhood\s*\(\s*(?:nodeType|type|kind)' `
  -AllowedRelativePaths @('frontend/src/components/dashboard/GraphStatsSection.tsx') `
  -Message "Graph citation/search code must not use node type counts as node ids"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern '\bsetTimeout\s*\(' `
  -AllowedRelativePaths @(
    'frontend/src/components/gql/GqlResultView.tsx',
    'frontend/src/components/tree/TreeContextMenu.tsx',
    'frontend/src/components/tree/TreeSearch.tsx'
  ) `
  -Message "Frontend runtime code must not fake business latency with setTimeout"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern 'Math\.random\s*\(' `
  -AllowedRelativePaths @(
    'frontend/src/components/graph/ForceGraph.tsx',
    'frontend/src/components/graph/graph-utils.ts',
    'frontend/src/app/components/ui/sidebar/sidebar-menu.tsx',
    'frontend/src/lib/saved-queries.ts'
  ) `
  -Message "Frontend runtime code must not use Math.random for business/mock data"

Write-Host "Frontend runtime guard passed: invoke boundary, runtime mock/demo residue, fake latency, and business random data are locked"
