$ErrorActionPreference = "Stop"

$patterns = @(
  "devtools",
  "internal-toggle-devtools",
  "mock-only secret"
)

$paths = @(
  "apps/desktop/src-tauri",
  "frontend/src"
)

$violations = @()
foreach ($path in $paths) {
  if (-not (Test-Path $path)) {
    continue
  }
  foreach ($pattern in $patterns) {
    $matches = Select-String -Path "$path/**/*" -Pattern $pattern -SimpleMatch -ErrorAction SilentlyContinue
    foreach ($match in $matches) {
      $violations += "$($match.Path):$($match.LineNumber): $($match.Line.Trim())"
    }
  }
}

if ($violations.Count -gt 0) {
  Write-Error "Release guard found debug-only strings:`n$($violations -join "`n")"
}

Write-Host "Release guard passed"
