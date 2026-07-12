param()

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$baselinePath = Join-Path $repoRoot 'scripts/baselines/rust-test-layout-baseline.csv'
$expectedHeader = 'path,inlineTestModules,inlineTestModuleLines,testAttributes,modTestsBlocks,testOnlyCfgItems,srcTestFileLines'
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

$baselineContent = Read-StrictUtf8 $baselinePath
$baselineLines = @(
  $baselineContent -split "`r?`n" |
    Where-Object { $_.Length -gt 0 }
)
if ($baselineLines.Count -ne 1 -or $baselineLines[0] -cne $expectedHeader) {
  $errors.Add(
    'Stage 6 requires rust-test-layout-baseline.csv to contain only its exact header'
  )
}

$workspaceRoots = @(
  (Join-Path $repoRoot 'crates')
  (Join-Path $repoRoot 'apps/desktop/src-tauri')
)
foreach ($workspaceRoot in $workspaceRoots) {
  foreach ($file in Get-ChildItem -LiteralPath $workspaceRoot -Recurse -File -Filter '*.rs') {
    $relative = $file.FullName.Substring($repoRoot.Length + 1).Replace('\', '/')
    if ($relative -notmatch '/src/') {
      continue
    }
    if ($file.Name -eq 'tests.rs' -or
        $file.Name -like '*_tests.rs' -or
        $relative -match '/src/tests/') {
      $errors.Add("test-only Rust source remains under src: $relative")
    }
  }
}

if ($errors.Count -gt 0) {
  Write-Error "Stage 6 test separation guard failed:`n$($errors -join "`n")"
}

& (Join-Path $repoRoot 'scripts/check-rust-test-layout.ps1')
if ($LASTEXITCODE -ne 0) {
  throw 'Stage 6 test separation guard failed: rust test-layout guard rejected the tree'
}

Write-Host 'Stage 6 test separation guard passed'
