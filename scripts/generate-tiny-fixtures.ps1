Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$rawDir = Join-Path $repoRoot 'testdata/fixtures/public-small/raw'
$rawPath = Join-Path $rawDir 'tiny.raw'
$e01Dir = Join-Path $repoRoot 'testdata/fixtures/public-small/e01'
$e01Path = Join-Path $e01Dir 'tiny.E01'
New-Item -ItemType Directory -Path $rawDir -Force | Out-Null
New-Item -ItemType Directory -Path $e01Dir -Force | Out-Null

$bytes = New-Object byte[] 1024
$signature = [System.Text.Encoding]::ASCII.GetBytes('FWB-TINY-RAW')
[Array]::Copy($signature, 0, $bytes, 512, $signature.Length)

# Minimal MBR marker so probe/parser tests can distinguish a sector-like image.
$bytes[510] = 0x55
$bytes[511] = 0xAA

[System.IO.File]::WriteAllBytes($rawPath, $bytes)
Write-Host "Wrote $rawPath ($($bytes.Length) bytes)"

function New-SectionDescriptor {
    param(
        [Parameter(Mandatory = $true)][string]$Type,
        [Parameter(Mandatory = $true)][UInt64]$Next,
        [Parameter(Mandatory = $true)][UInt64]$Size
    )

    $descriptor = New-Object byte[] 76
    $typeBytes = [System.Text.Encoding]::ASCII.GetBytes($Type)
    [Array]::Copy($typeBytes, 0, $descriptor, 0, [Math]::Min($typeBytes.Length, 16))
    [Array]::Copy([BitConverter]::GetBytes($Next), 0, $descriptor, 16, 8)
    [Array]::Copy([BitConverter]::GetBytes($Size), 0, $descriptor, 24, 8)
    return $descriptor
}

# Minimal deterministic single-segment E01 fixture. It is intentionally not a
# full filesystem image; it verifies E01 section walking, table mapping, read,
# and seek behavior without depending on private workstation evidence.
$chunkSectors = [UInt32]8
$sectors = [UInt64]8
$chunkBytes = [int]($chunkSectors * 512)
$fileHeader = [byte[]](0x45, 0x56, 0x46, 0x09, 0x0D, 0x0A, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00)

$volumeDescOffset = [UInt64]13
$volumeSize = [UInt64]36
$tableDescOffset = [UInt64]($volumeDescOffset + 76 + $volumeSize)
$tableSize = [UInt64]32
$doneDescOffset = [UInt64]($tableDescOffset + 76 + $tableSize)
$dataStart = [UInt64]($doneDescOffset + 76)

$volume = New-Object byte[] $volumeSize
[Array]::Copy([BitConverter]::GetBytes($chunkSectors), 0, $volume, 8, 4)
[Array]::Copy([BitConverter]::GetBytes($chunkSectors), 0, $volume, 12, 4)
[Array]::Copy([BitConverter]::GetBytes($sectors), 0, $volume, 16, 8)

$table = New-Object byte[] $tableSize
[Array]::Copy([BitConverter]::GetBytes([UInt32]1), 0, $table, 0, 4)
[Array]::Copy([BitConverter]::GetBytes($dataStart), 0, $table, 8, 8)

$chunk = New-Object byte[] $chunkBytes
$chunkMarker = [System.Text.Encoding]::ASCII.GetBytes('FWB-TINY-E01')
[Array]::Copy($chunkMarker, 0, $chunk, 0, $chunkMarker.Length)
$chunk[510] = 0x55
$chunk[511] = 0xAA

$stream = [System.IO.MemoryStream]::new()
try {
    $stream.Write($fileHeader, 0, $fileHeader.Length)
    $desc = New-SectionDescriptor -Type 'volume' -Next $tableDescOffset -Size ([UInt64](76 + $volumeSize))
    $stream.Write($desc, 0, $desc.Length)
    $stream.Write($volume, 0, $volume.Length)
    $desc = New-SectionDescriptor -Type 'table' -Next $doneDescOffset -Size ([UInt64](76 + $tableSize))
    $stream.Write($desc, 0, $desc.Length)
    $stream.Write($table, 0, $table.Length)
    $desc = New-SectionDescriptor -Type 'done' -Next 0 -Size 0
    $stream.Write($desc, 0, $desc.Length)
    $stream.Write($chunk, 0, $chunk.Length)
    [System.IO.File]::WriteAllBytes($e01Path, $stream.ToArray())
} finally {
    $stream.Dispose()
}
Write-Host "Wrote $e01Path ($((Get-Item $e01Path).Length) bytes)"

$cargo = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
& $cargo run --quiet -p testing --bin generate_tiny_registry_fixtures -- $repoRoot
if ($LASTEXITCODE -ne 0) {
    throw "Registry fixture generation failed with exit code $LASTEXITCODE"
}
