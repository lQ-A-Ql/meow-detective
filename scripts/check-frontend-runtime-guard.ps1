param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$frontendSrc = Join-Path $repoRoot "frontend/src"
$apiClientPath = Join-Path $frontendSrc "lib/api/client.ts"
$pagesPath = Join-Path $frontendSrc "app/pages"

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

function Assert-NoRawControlsOutsideAllowedCases {
  param([Parameter(Mandatory = $true)][System.IO.FileInfo[]]$Files)

  $primitiveAllowedPaths = @(
    'frontend/src/app/components/ui/input.tsx',
    'frontend/src/app/components/ui/textarea.tsx',
    'frontend/src/app/components/ui/checkbox.tsx',
    'frontend/src/app/components/ui/select.tsx',
    'frontend/src/app/components/ui/table.tsx'
  )
  $mediaRangeAllowedPaths = @(
    'frontend/src/components/viewers/AudioViewer.tsx',
    'frontend/src/components/viewers/VideoViewer.tsx'
  )

  $violations = @()
  foreach ($file in $Files) {
    $relative = Get-RelativePath $file.FullName
    $content = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8
    $matches = [regex]::Matches($content, '<\s*(input|textarea|select|table)\b')

    foreach ($match in $matches) {
      $lineNumber = ($content.Substring(0, $match.Index) -split "`n").Count
      $line = ($content -split "`n")[$lineNumber - 1].Trim()
      $remaining = $content.Substring($match.Index)
      $tagEnd = $remaining.IndexOf('>')
      $tagText = if ($tagEnd -ge 0) { $remaining.Substring(0, $tagEnd + 1) } else { $line }
      $tagName = $match.Groups[1].Value.ToLowerInvariant()

      $isAllowed = $false
      if ($primitiveAllowedPaths -contains $relative) {
        $isAllowed = $true
      } elseif (
        $relative -eq 'frontend/src/features/notebook/components/helpers.tsx' -and
        $tagName -eq 'input' -and
        $tagText -match 'type\s*=\s*["'']checkbox["'']' -and
        $tagText -match '\bdisabled\b'
      ) {
        $isAllowed = $true
      } elseif (
        $mediaRangeAllowedPaths -contains $relative -and
        $tagName -eq 'input' -and
        $tagText -match 'type\s*=\s*["'']range["'']'
      ) {
        $isAllowed = $true
      }

      if (-not $isAllowed) {
        $violations += "{0}:{1}: {2}" -f $relative, $lineNumber, $line
      }
    }
  }

  if ($violations.Count -gt 0) {
    throw "Frontend runtime UI must use the standard Input/Textarea/Select/Checkbox/DenseDataTable primitives instead of raw form/table controls`n$($violations -join "`n")"
  }
}

function Assert-NoRawButtonsOutsideAllowedCases {
  param([Parameter(Mandatory = $true)][System.IO.FileInfo[]]$Files)

  $allowedPaths = @(
    'frontend/src/app/components/ui/button.tsx',
    'frontend/src/app/components/ui/sidebar/sidebar-group.tsx',
    'frontend/src/app/components/ui/sidebar/sidebar-menu.tsx',
    'frontend/src/app/components/ui/sidebar/sidebar-rail.tsx'
  )

  $violations = @()
  foreach ($file in $Files) {
    $relative = Get-RelativePath $file.FullName
    if ($allowedPaths -contains $relative) {
      continue
    }

    $content = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8
    $matches = [regex]::Matches($content, '<\s*button\b')
    foreach ($match in $matches) {
      $lineNumber = ($content.Substring(0, $match.Index) -split "`n").Count
      $line = ($content -split "`n")[$lineNumber - 1].Trim()
      $violations += "{0}:{1}: {2}" -f $relative, $lineNumber, $line
    }
  }

  if ($violations.Count -gt 0) {
    throw "Frontend runtime UI must use the shared Button primitive instead of raw button elements`n$($violations -join "`n")"
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

function Assert-NoRuntimeFilesUnder {
  param(
    [Parameter(Mandatory = $true)][string]$RelativePath,
    [Parameter(Mandatory = $true)][string]$Message
  )

  $absolute = Join-Path $repoRoot $RelativePath
  if (-not (Test-Path -LiteralPath $absolute)) {
    return
  }

  $files = @(Get-ChildItem -LiteralPath $absolute -Recurse -File -Include *.ts,*.tsx |
    Where-Object { -not (Is-TestOrFixtureFile $_) })
  if ($files.Count -gt 0) {
    $relativeFiles = $files | ForEach-Object { Get-RelativePath $_.FullName }
    throw "$Message`n$($relativeFiles -join "`n")"
  }
}

$runtimeFiles = @(Get-ChildItem -LiteralPath $frontendSrc -Recurse -File -Include *.ts,*.tsx |
  Where-Object { -not (Is-TestOrFixtureFile $_) })

$pageRuntimeFiles = @(Get-ChildItem -LiteralPath $pagesPath -Recurse -File -Include *.ts,*.tsx |
  Where-Object { -not (Is-TestOrFixtureFile $_) })

$featureRuntimeRelativePaths = @(Get-ChildItem -LiteralPath (Join-Path $frontendSrc "features") -Recurse -File -Include *.ts,*.tsx |
  Where-Object { -not (Is-TestOrFixtureFile $_) } |
  ForEach-Object { Get-RelativePath $_.FullName })

$storeRuntimeRelativePaths = @(Get-ChildItem -LiteralPath (Join-Path $frontendSrc "stores") -Recurse -File -Include *.ts,*.tsx |
  Where-Object { -not (Is-TestOrFixtureFile $_) } |
  ForEach-Object { Get-RelativePath $_.FullName })

$sharedComponentAdapterAllowedPaths = @(
  'frontend/src/components/layout/AppShell.tsx',
  'frontend/src/components/layout/BottomDrawer.tsx',
  'frontend/src/components/layout/TopBar.tsx',
  'frontend/src/lib/api/client.ts',
  'frontend/src/lib/events/tauri-bridge.ts',
  'frontend/src/lib/platform/dialog.ts'
) + $featureRuntimeRelativePaths + $storeRuntimeRelativePaths

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
  -Pattern "['""]@tauri-apps/" `
  -AllowedRelativePaths @(
    'frontend/src/lib/api/client.ts',
    'frontend/src/lib/events/tauri-bridge.ts',
    'frontend/src/lib/platform/dialog.ts'
  ) `
  -Message "Frontend runtime code must not import Tauri packages outside platform/API/event adapters"

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
  -Pattern '(?i)\bmock\b|\bdemo\b|示例|演示' `
  -Message "Frontend production source must not contain mock/demo/example presentation residue"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern "from\s+['""]@/lib/api/(?!client['""]|mcp['""])[^'""]+['""]" `
  -AllowedRelativePaths @(Get-ChildItem -LiteralPath (Join-Path $frontendSrc "features") -Recurse -File -Include *.ts,*.tsx |
    ForEach-Object { Get-RelativePath $_.FullName }) `
  -Message "Frontend pages/components/stores must not import business API modules directly; route through feature hooks"

Assert-NoMatchesInRuntimeFiles `
  -Files $pageRuntimeFiles `
  -Pattern "from\s+['""]@/(?:lib/api|lib/platform|stores)/|from\s+['""]@tauri-apps/" `
  -Message "Frontend page route shells must not import API, platform adapters, stores, or Tauri directly"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern "from\s+['""]@/app/pages/|from\s+['""]\.\.?/[^'""]*app/pages/" `
  -Message "Frontend runtime code must not import page-private modules"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/analysis' `
  -Message "Analysis domain components belong under frontend/src/features/analysis/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/batch' `
  -Message "Batch domain components belong under frontend/src/features/batch/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/dashboard' `
  -Message "Dashboard domain components belong under frontend/src/features/dashboard/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/files' `
  -Message "Files domain components belong under frontend/src/features/files/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/gql' `
  -Message "GQL domain components belong under frontend/src/features/gql/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/graph' `
  -Message "Graph domain components belong under frontend/src/features/graph/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/import' `
  -Message "Import domain components belong under frontend/src/features/import/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/marketplace' `
  -Message "Marketplace domain components belong under frontend/src/features/marketplace/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/mcp' `
  -Message "MCP domain components belong under frontend/src/features/mcp/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/notebook' `
  -Message "Notebook domain components belong under frontend/src/features/notebook/components"

Assert-NoRuntimeFilesUnder `
  -RelativePath 'frontend/src/components/rule-packs' `
  -Message "Rule pack domain components belong under frontend/src/features/rule-packs/components"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern "from\s+['""]@/(?:lib/api|lib/platform|stores)/|from\s+['""]@tauri-apps/" `
  -AllowedRelativePaths $sharedComponentAdapterAllowedPaths `
  -Message "Shared components and pages must not import API, platform adapters, stores, or Tauri directly; move containers to feature layer"

Assert-NoRawControlsOutsideAllowedCases -Files $runtimeFiles
Assert-NoRawButtonsOutsideAllowedCases -Files $runtimeFiles

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern "from\s+['""]@/app/components/ui/table['""]" `
  -AllowedRelativePaths @('frontend/src/components/tables/DenseDataTable.tsx') `
  -Message "Feature/page code must not import low-level Table primitives directly; use DenseDataTable or a semantic summary component"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern 'Object\.keys\s*\(\s*[^)]*nodeCountByType[^)]*\)|getNodeNeighborhood\s*\(\s*(?:nodeType|type|kind)' `
  -AllowedRelativePaths @('frontend/src/features/dashboard/components/GraphStatsSection.tsx') `
  -Message "Graph citation/search code must not use node type counts as node ids"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern '\bsetTimeout\s*\(' `
  -AllowedRelativePaths @(
    'frontend/src/features/gql/components/GqlResultView.tsx',
    'frontend/src/components/tree/TreeContextMenu.tsx',
    'frontend/src/components/tree/TreeSearch.tsx'
  ) `
  -Message "Frontend runtime code must not fake business latency with setTimeout"

Assert-NoMatchesInRuntimeFiles `
  -Files $runtimeFiles `
  -Pattern 'Math\.random\s*\(' `
  -AllowedRelativePaths @(
    'frontend/src/features/graph/components/ForceGraph.tsx',
    'frontend/src/features/graph/components/graph-utils.ts',
    'frontend/src/app/components/ui/sidebar/sidebar-menu.tsx',
    'frontend/src/lib/saved-queries.ts'
  ) `
  -Message "Frontend runtime code must not use Math.random for business/mock data"

Write-Host "Frontend runtime guard passed: invoke boundary, runtime mock/demo residue, fake latency, and business random data are locked"
