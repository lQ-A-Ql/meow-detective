param()
$ErrorActionPreference = 'Stop'
$parserRepoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $PSScriptRoot 'baselines/parser-ownership.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
$ownershipFailures = New-Object System.Collections.Generic.List[string]
foreach ($format in $manifest.formats) {
    foreach ($owner in $format.owners) {
        if (-not (Test-Path -LiteralPath (Join-Path $parserRepoRoot $owner) -PathType Leaf)) {
            $ownershipFailures.Add("$($format.id) owner is missing: $owner")
        }
    }
    foreach ($retired in $format.retired) {
        if (Test-Path -LiteralPath (Join-Path $parserRepoRoot $retired)) {
            $ownershipFailures.Add("$($format.id) retired implementation is present: $retired")
        }
    }
    foreach ($reference in $format.references) {
        $referencePath = Join-Path $parserRepoRoot $reference.path
        if (-not (Test-Path -LiteralPath $referencePath -PathType Leaf)) {
            $ownershipFailures.Add("$($format.id) caller is missing: $($reference.path)")
            continue
        }
        $referenceText = Get-Content -LiteralPath $referencePath -Raw -Encoding UTF8
        if (-not $referenceText.Contains($reference.marker)) {
            $ownershipFailures.Add("$($format.id) shared owner reference is missing: $($reference.path)")
        }
    }
}
if ($ownershipFailures.Count -gt 0) {
    throw ($ownershipFailures -join [Environment]::NewLine)
}
Write-Output "Parser ownership guard passed: $($manifest.formats.Count) format responsibilities and retired implementation paths"
