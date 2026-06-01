param(
  [datetime]$AsOfDate = (Get-Date).Date
)

$ErrorActionPreference = "Stop"

$denyPath = Join-Path $PSScriptRoot "..\deny.toml"
if (-not (Test-Path -LiteralPath $denyPath)) {
  throw "deny.toml not found at $denyPath"
}

$content = Get-Content -LiteralPath $denyPath -Raw -Encoding UTF8
$entries = [regex]::Matches(
  $content,
  '\{[^{}]*id\s*=\s*"(?<id>RUSTSEC-\d{4}-\d{4})"[^{}]*\}',
  [System.Text.RegularExpressions.RegexOptions]::Singleline
)

if ($entries.Count -eq 0) {
  Write-Host "Dependency exception guard passed: no advisory exceptions configured"
  exit 0
}

$violations = @()
foreach ($entry in $entries) {
  $id = $entry.Groups["id"].Value
  $reasonMatch = [regex]::Match(
    $entry.Value,
    'reason\s*=\s*"(?<reason>(?:[^"\\]|\\.)*)"',
    [System.Text.RegularExpressions.RegexOptions]::Singleline
  )

  if (-not $reasonMatch.Success) {
    $violations += "$id missing reason field"
    continue
  }

  $reason = $reasonMatch.Groups["reason"].Value
  if ($reason -notmatch '(?i)\bowner\s*:') {
    $violations += "$id reason missing owner"
  }

  $expiryMatch = [regex]::Match($reason, '(?i)\bexpires\s*:\s*(?<date>\d{4}-\d{2}-\d{2})')
  if (-not $expiryMatch.Success) {
    $violations += "$id reason missing expires: YYYY-MM-DD"
    continue
  }

  $expiry = [datetime]::ParseExact(
    $expiryMatch.Groups["date"].Value,
    "yyyy-MM-dd",
    [System.Globalization.CultureInfo]::InvariantCulture
  ).Date

  if ($expiry -lt $AsOfDate.Date) {
    $violations += "$id expired on $($expiry.ToString('yyyy-MM-dd'))"
  }

  $nonMetadata = $reason -replace '(?i)owner\s*:[^;]+;?', ''
  $nonMetadata = $nonMetadata -replace '(?i)expires\s*:\s*\d{4}-\d{2}-\d{2};?', ''
  if ([string]::IsNullOrWhiteSpace($nonMetadata)) {
    $violations += "$id reason missing explanatory text"
  }
}

if ($violations.Count -gt 0) {
  Write-Error "Dependency exception guard failed:`n$($violations -join "`n")"
}

Write-Host "Dependency exception guard passed: $($entries.Count) advisory exceptions checked"
