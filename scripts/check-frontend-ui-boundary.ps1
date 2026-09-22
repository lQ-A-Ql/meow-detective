[CmdletBinding()]
param(
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$scanRoots = @(
  (Join-Path $root 'frontend/src/app/pages'),
  (Join-Path $root 'frontend/src/features')
)
$forbidden = @('<button', '<select', '<textarea', '<input')
$violations = @()

foreach ($scanRoot in $scanRoots) {
  if (-not (Test-Path -LiteralPath $scanRoot)) { continue }
  Get-ChildItem -LiteralPath $scanRoot -Recurse -File -Include *.tsx,*.ts | ForEach-Object {
    $path = $_.FullName
    $lineNumber = 0
    Get-Content -LiteralPath $path | ForEach-Object {
      $lineNumber++
      foreach ($token in $forbidden) {
        if ($_ -cmatch [regex]::Escape($token)) {
          $violations += "${path}:$lineNumber contains forbidden native control '$token'"
        }
      }
    }
  }
}

$pageRoot = Join-Path $root 'frontend/src/app/pages'
if (Test-Path -LiteralPath $pageRoot) {
  Get-ChildItem -LiteralPath $pageRoot -Recurse -File -Include *.tsx,*.ts |
    Where-Object { $_.Name -notmatch '\.test\.(tsx|ts)$' } |
    ForEach-Object {
      $path = $_.FullName
      $lineNumber = 0
      Get-Content -LiteralPath $path | ForEach-Object {
        $lineNumber++
        foreach ($token in @('useQuery', 'useMutation', 'apiClient', 'invoke(')) {
          if ($_ -cmatch [regex]::Escape($token)) {
            $violations += "${path}:$lineNumber page contains logic boundary token '$token'"
          }
        }
      }
    }
}

$forbiddenPatterns = @(
  'dangerouslySetInnerHTML',
  '#[0-9A-Fa-f]{3,8}',
  'style\s*=\s*\{\{[^}]*color'
)
$patternRoots = @(
  (Join-Path $root 'frontend/src/features'),
  (Join-Path $root 'frontend/src/app/pages'),
  (Join-Path $root 'frontend/src/lib')
)
foreach ($patternRoot in $patternRoots) {
  if (-not (Test-Path -LiteralPath $patternRoot)) { continue }
  Get-ChildItem -LiteralPath $patternRoot -Recurse -File -Include *.tsx,*.ts | Where-Object {
    $_.FullName -notmatch '\.test\.(tsx|ts)$'
  } | ForEach-Object {
    $path = $_.FullName
    $lineNumber = 0
    Get-Content -LiteralPath $path | ForEach-Object {
      $lineNumber++
      foreach ($pattern in $forbiddenPatterns) {
        if ($_ -match $pattern) {
          $violations += "${path}:$lineNumber matches forbidden visual/security pattern '$pattern'"
        }
      }
    }
  }
}

if ($SelfTest) {
  if ($violations.Count -ne 0) {
    throw "Frontend UI boundary self-test failed: $($violations -join '; ')"
  }
  Write-Output 'Frontend UI boundary self-test passed.'
  exit 0
}

if ($violations.Count -ne 0) {
  $violations | ForEach-Object { Write-Error $_ }
  exit 1
}
Write-Output 'Frontend UI boundary passed.'
