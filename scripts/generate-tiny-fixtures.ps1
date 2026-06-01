Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$rawDir = Join-Path $repoRoot 'testdata/fixtures/tiny/raw'
$rawPath = Join-Path $rawDir 'tiny.raw'
New-Item -ItemType Directory -Path $rawDir -Force | Out-Null

$bytes = New-Object byte[] 1024
$signature = [System.Text.Encoding]::ASCII.GetBytes('FWB-TINY-RAW')
[Array]::Copy($signature, 0, $bytes, 512, $signature.Length)

# Minimal MBR marker so probe/parser tests can distinguish a sector-like image.
$bytes[510] = 0x55
$bytes[511] = 0xAA

[System.IO.File]::WriteAllBytes($rawPath, $bytes)
Write-Host "Wrote $rawPath ($($bytes.Length) bytes)"
