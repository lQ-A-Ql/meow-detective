[CmdletBinding()]
param(
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$scanRoots = @(
  (Join-Path $root 'frontend/src/app/pages'),
  (Join-Path $root 'frontend/src/features/infrastructure')
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
