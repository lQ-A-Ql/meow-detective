param(
  [switch]$StrictBackend
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$violations = @()
$backendWarnings = @()

function Read-RepoFile {
  param([Parameter(Mandatory = $true)][string]$RelativePath)

  $path = Join-Path $repoRoot $RelativePath
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required file is missing: $RelativePath"
  }

  return Get-Content -LiteralPath $path -Raw -Encoding UTF8
}

function Get-RelativePath {
  param([Parameter(Mandatory = $true)][string]$FullName)

  return $FullName.Substring($repoRoot.Path.Length).TrimStart([char[]]@('\', '/')) -replace '\\', '/'
}

function Remove-CfgTestModules {
  param([Parameter(Mandatory = $true)][string]$Content)

  $pattern = [regex]'#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{'

  while ($true) {
    $match = $pattern.Match($Content)
    if (-not $match.Success) { break }

    $start = $match.Index
    $i = $match.Index + $match.Length - 1
    $depth = 0
    $end = -1

    while ($i -lt $Content.Length) {
      $ch = $Content[$i]
      if ($ch -eq '{') {
        $depth++
      }
      elseif ($ch -eq '}') {
        $depth--
        if ($depth -eq 0) {
          $end = $i
          break
        }
      }
      $i++
    }

    if ($end -lt 0) { break }

    $Content = $Content.Substring(0, $start) + $Content.Substring($end + 1)
  }

  return $Content
}

function Add-RegexViolations {
  param(
    [Parameter(Mandatory = $true)][string]$RelativePath,
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][array]$Rules
  )

  $lines = $Content -split "\r?\n"
  foreach ($rule in $Rules) {
    $matches = [regex]::Matches($Content, $rule.Pattern)
    foreach ($match in $matches) {
      $lineNumber = ($Content.Substring(0, $match.Index) -split "`n").Count
      $line = $lines[$lineNumber - 1].Trim()
      $script:violations += "{0}:{1}: {2}: {3}" -f $RelativePath, $lineNumber, $rule.Label, $line
    }
  }
}

function Find-RegexViolations {
  param(
    [Parameter(Mandatory = $true)][string]$RelativePath,
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][array]$Rules
  )

  $findings = @()
  $lines = $Content -split "\r?\n"
  foreach ($rule in $Rules) {
    $matches = [regex]::Matches($Content, $rule.Pattern)
    foreach ($match in $matches) {
      $lineNumber = ($Content.Substring(0, $match.Index) -split "`n").Count
      $line = $lines[$lineNumber - 1].Trim()
      $findings += "{0}:{1}: {2}: {3}" -f $RelativePath, $lineNumber, $rule.Label, $line
    }
  }

  return $findings
}

function Is-RustTestFile {
  param([Parameter(Mandatory = $true)][System.IO.FileInfo]$File)

  return $File.Name -match '(^tests\.rs$|_tests\.rs$|_test\.rs$|\.test\.rs$)'
}

function Is-FrontendTestFile {
  param([Parameter(Mandatory = $true)][System.IO.FileInfo]$File)

  $relative = Get-RelativePath $File.FullName
  return $File.Name -match '\.(test|spec)\.(ts|tsx)$' `
    -or $relative -match '(^|/)test/' `
    -or $relative -match '(^|/)__tests__/'
}

# Stage 0 records the backend event-emission boundary as a known Stage 1
# hardening item. Use -StrictBackend once Stage 1 starts to make these findings
# fatal instead of advisory.
$appServicesSrc = Join-Path $repoRoot "crates/app-services/src"
if (-not (Test-Path -LiteralPath $appServicesSrc)) {
  throw "Required app-services source root is missing: $appServicesSrc"
}

$appServiceRules = @(
  @{ Pattern = '\btauri::'; Label = 'tauri runtime usage' },
  @{ Pattern = '(?m)^\s*use\s+tauri\b'; Label = 'tauri import' },
  @{ Pattern = '\bAppHandle\b'; Label = 'Tauri AppHandle dependency' },
  @{ Pattern = '\.emit_to\s*\('; Label = 'Tauri emit_to dependency' }
)

Get-ChildItem -LiteralPath $appServicesSrc -Recurse -File -Filter '*.rs' |
  Where-Object { -not (Is-RustTestFile $_) } |
  ForEach-Object {
    $content = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8
    $content = Remove-CfgTestModules $content
    $findings = @(Find-RegexViolations (Get-RelativePath $_.FullName) $content $appServiceRules)
    if ($findings.Count -gt 0) {
      if ($StrictBackend) {
        $violations += $findings
      } else {
        $backendWarnings += $findings
      }
    }
  }

# Stage 0 frontend boundary: production UI code must not pin a private fixture
# case id or bypass the central Tauri API client.
$frontendSrc = Join-Path $repoRoot "frontend/src"
if (-not (Test-Path -LiteralPath $frontendSrc)) {
  throw "Required frontend source root is missing: $frontendSrc"
}

$frontendProductionFiles = Get-ChildItem -LiteralPath $frontendSrc -Recurse -File |
  Where-Object { $_.Extension -in @(".ts", ".tsx") } |
  Where-Object { -not (Is-FrontendTestFile $_) }

foreach ($file in $frontendProductionFiles) {
  $relative = Get-RelativePath $file.FullName
  $content = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8

  Add-RegexViolations $relative $content @(
    @{ Pattern = 'case-2026-fx-091'; Label = 'hard-coded private fixture case id' }
  )

  if ($relative -ne "frontend/src/lib/api/client.ts") {
    Add-RegexViolations $relative $content @(
      @{ Pattern = '@tauri-apps/api/core'; Label = 'direct Tauri API import outside api client' },
      @{ Pattern = '\binvoke\s*(?:<[^>\r\n]+>)?\s*\('; Label = 'direct invoke call outside api client' }
    )
  }
}

# Stage 0 media command-name boundary: files.ts must consume the central command
# map instead of repeating bare Tauri command strings.
$frontendFilesApi = Read-RepoFile "frontend/src/lib/api/files.ts"

if ($frontendFilesApi -notmatch 'export\s+async\s+function\s+getMediaUrl\s*\([^)]*\)[\s\S]{0,800}?apiClient\.request<MediaUrl>\(\s*COMMANDS\.files\.GET_MEDIA_URL') {
  $violations += "frontend/src/lib/api/files.ts: getMediaUrl must call apiClient.request with COMMANDS.files.GET_MEDIA_URL"
}

if ($frontendFilesApi -notmatch 'export\s+async\s+function\s+readMediaRange\s*\([^)]*\)[\s\S]{0,800}?apiClient\.request<MediaRangeResponse>\(\s*COMMANDS\.files\.READ_MEDIA_RANGE') {
  $violations += "frontend/src/lib/api/files.ts: readMediaRange must call apiClient.request with COMMANDS.files.READ_MEDIA_RANGE"
}

if ($frontendFilesApi -match 'apiClient\.request(?:<[^>]+>)?\(\s*[''"](get_media_url|read_media_range)[''"]') {
  $violations += "frontend/src/lib/api/files.ts: media commands must not be passed as bare string literals"
}

if ($violations.Count -gt 0) {
  Write-Error "Stage 0 boundary guard failed:`n$($violations -join "`n")"
}

if ($backendWarnings.Count -gt 0) {
  Write-Warning "Stage 1 backend boundary findings are currently advisory. Re-run with -StrictBackend to fail on:`n$($backendWarnings -join "`n")"
}

Write-Host "Stage 0 boundary guard passed: frontend fixture id, media COMMANDS, and invoke boundaries are locked"
