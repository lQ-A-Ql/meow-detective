$ErrorActionPreference = "Stop"

$commandRoot = Join-Path $PSScriptRoot "..\apps\desktop\src-tauri\src\commands"
if (-not (Test-Path -LiteralPath $commandRoot)) {
  throw "Command root not found at $commandRoot"
}

$patterns = @(
  '\bSELECT\b',
  '\bINSERT\b',
  '\bUPDATE\b',
  '\bDELETE\b',
  '\bCREATE\b',
  '\bALTER\b',
  '\bDROP\b',
  'rusqlite::params!',
  '\.prepare\s*\(',
  '\.execute\s*\('
)

$files = Get-ChildItem -LiteralPath $commandRoot -Recurse -File -Include *.rs
$violations = @()
foreach ($file in $files) {
  $lines = Get-Content -LiteralPath $file.FullName -Encoding UTF8
  $content = $lines -join "`n"
  foreach ($pattern in $patterns) {
    $matches = [regex]::Matches($content, $pattern)
    foreach ($match in $matches) {
      $lineNumber = ($content.Substring(0, $match.Index) -split "`n").Count
      $line = $lines[$lineNumber - 1].Trim()
      $violations += "{0}:{1}: {2}" -f $file.FullName, $lineNumber, $line
    }
  }
}

if ($violations.Count -gt 0) {
  Write-Error "Command SQL boundary guard failed. Move business SQL into repository/service layers:`n$($violations -join "`n")"
}

Write-Host "Command SQL boundary guard passed"
