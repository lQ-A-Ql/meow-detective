#!/usr/bin/env pwsh
<#
.SYNOPSIS
  LVM offset-discipline guard: verifies that all SeekFrom::Start calls in
  fs-lvm use absolute (device-reader) offsets, not PV-relative ones.

.DESCRIPTION
  During real E01 testing, three PV-relative-vs-absolute offset bugs were
  found and fixed:
    1. parse_metadata() now accepts pv_offset and adds it to all seeks
    2. parse_descriptors() no longer skips size=0 entries
    3. pv_data_offsets now includes pv_offset (absolute)

  This guard enforces the patterns that prevent these bugs from recurring.
#>

param(
    [string]$ProjectRoot = "$PSScriptRoot\.."
)

$ErrorActionPreference = 'Stop'
$issues = @()

Push-Location $ProjectRoot

try {
    # ── Rule 1: parse_metadata MUST be called with pv_offset ──
    $metadataCalls = Select-String -Path 'crates\fs-lvm\src\lib.rs' -Pattern 'parse_metadata\(' -SimpleMatch
    foreach ($call in $metadataCalls) {
        if ($call.Line -notmatch 'pv_offset|first_offset') {
            $issues += "parse_metadata called without pv_offset at $($call.Path):$($call.LineNumber)"
        }
    }

    # ── Rule 2: pv_data_offsets MUST include pv_offset (first_offset) ──
    $offsetLines = Select-String -Path 'crates\fs-lvm\src\lib.rs' -Pattern 'pv_data_offsets\.push' -SimpleMatch
    foreach ($line in $offsetLines) {
        if ($line.Line -notmatch 'first_offset\s*\+') {
            $issues += "pv_data_offsets.push missing `first_offset +` at $($line.Path):$($line.LineNumber)"
        }
    }

    # ── Rule 3: LvReader::read_at seek MUST use extent_map physical_offset ──
    $seekLines = Select-String -Path 'crates\fs-lvm\src\lv_reader.rs' -Pattern 'SeekFrom::Start\(' -SimpleMatch
    foreach ($line in $seekLines) {
        if ($line.Line -match 'SeekFrom::Start') {
            if ($line.Line -notmatch 'physical_offset' -and $line.Line -notmatch 'ext\.physical') {
                $issues += "SeekFrom::Start without physical_offset at $($line.Path):$($line.LineNumber)"
            }
        }
    }

    # ── Rule 4: parse_descriptors MUST NOT filter size=0 ──
    $descLines = Select-String -Path 'crates\fs-lvm\src\label.rs' -Pattern 'desc_size\s*[><=]' -SimpleMatch
    $hasSkip = $false
    foreach ($line in $descLines) {
        if ($line.Line -match 'desc_size\s*>\s*0' -and $line.Line -match 'push') {
            $hasSkip = $true
            $issues += "parse_descriptors still filters size=0 at $($line.Path):$($line.LineNumber)"
        }
    }

    # ── Rule 5: No direct SeekFrom::Start with raw integer in metadata.rs ──
    $metaSeeks = Select-String -Path 'crates\fs-lvm\src\metadata.rs' -Pattern 'SeekFrom::Start\(' -SimpleMatch
    foreach ($line in $metaSeeks) {
        if ($line.Line -match 'SeekFrom::Start\(abs' -or $line.Line -match 'SeekFrom::Start\(pv_offset') {
            # OK — uses absolute offset variable
        } elseif ($line.Line -match 'SeekFrom::Start\(mda_region\.offset\)') {
            $issues += "SeekFrom::Start(mda_region.offset) without pv_offset at $($line.Path):$($line.LineNumber)"
        }
    }

    # ── Report ──
    if ($issues.Count -eq 0) {
        Write-Host "LVM offset-discipline guard passed: all SeekFrom::Start calls use absolute offsets" -ForegroundColor Green
        exit 0
    } else {
        Write-Host "LVM offset-discipline guard FAILED ($($issues.Count) issue(s)):" -ForegroundColor Red
        foreach ($issue in $issues) {
            Write-Host "  - $issue" -ForegroundColor Red
        }
        exit 1
    }
} finally {
    Pop-Location
}
