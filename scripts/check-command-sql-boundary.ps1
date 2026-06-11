$ErrorActionPreference = "Stop"

$commandRoot = Join-Path $PSScriptRoot "..\apps\desktop\src-tauri\src\commands"
if (-not (Test-Path -LiteralPath $commandRoot)) {
  throw "Command root not found at $commandRoot"
}

# This guard protects a layering boundary: Tauri command handlers must stay thin
# IPC adapters and must not embed business SQL (see CLAUDE.md "command handlers
# should validate/translate request DTOs and delegate, not implement business
# workflows or SQL directly").
#
# `#[cfg(test)] mod <name> { ... }` blocks are compiled out of release builds and
# are NOT command-handler logic: integration tests legitimately use raw SQL to
# seed fixtures and assert row counts against the database. They are outside this
# boundary, so strip them before scanning. Production code (including any inline
# `#[cfg(test)]` statements outside a test module) is still fully scanned.
function Remove-CfgTestModules {
  param([string]$content)

  # `#[cfg(test)]` (optionally followed by more attributes) then `mod NAME {`.
  $pattern = [regex]'#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{'

  while ($true) {
    $match = $pattern.Match($content)
    if (-not $match.Success) { break }

    $start = $match.Index
    # Index of the module's opening brace (last char of the matched header).
    $i = $match.Index + $match.Length - 1
    $depth = 0
    $end = -1
    while ($i -lt $content.Length) {
      $ch = $content[$i]
      if ($ch -eq '{') {
        $depth++
      }
      elseif ($ch -eq '}') {
        $depth--
        if ($depth -eq 0) { $end = $i; break }
      }
      $i++
    }

    # Unbalanced braces: refuse to strip so the guard fails toward a visible
    # false positive rather than silently skipping real production code.
    if ($end -lt 0) { break }

    $content = $content.Substring(0, $start) + $content.Substring($end + 1)
  }

  return $content
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
  $rawContent = (Get-Content -LiteralPath $file.FullName -Encoding UTF8) -join "`n"
  $content = Remove-CfgTestModules $rawContent
  $lines = $content -split "`n"
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
