param(
    [string]$OutputDir = (Join-Path ([System.IO.Path]::GetTempPath()) 'forensics-webview2-media-smoke'),
    [switch]$SkipProtocolGuard
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$outputPath = New-Item -ItemType Directory -Path $OutputDir -Force

# --- helper functions ---

function Write-UInt16Le {
    param(
        [Parameter(Mandatory = $true)][System.IO.BinaryWriter]$Writer,
        [Parameter(Mandatory = $true)][UInt16]$Value
    )
    $Writer.Write([BitConverter]::GetBytes($Value))
}

function Write-UInt32Le {
    param(
        [Parameter(Mandatory = $true)][System.IO.BinaryWriter]$Writer,
        [Parameter(Mandatory = $true)][UInt32]$Value
    )
    $Writer.Write([BitConverter]::GetBytes($Value))
}

function Write-Ascii {
    param(
        [Parameter(Mandatory = $true)][System.IO.BinaryWriter]$Writer,
        [Parameter(Mandatory = $true)][string]$Text
    )
    $Writer.Write([System.Text.Encoding]::ASCII.GetBytes($Text))
}

function Write-WavPcm16Mono {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][UInt32]$DataBytes
    )
    if (($DataBytes % 2) -ne 0) { throw "DataBytes must be even for PCM16 mono" }
    $sr = [UInt32]44100; $bps = [UInt16]16; $ch = [UInt16]1
    $ba = [UInt16](($ch * $bps) / 8); $br = [UInt32]($sr * $ba)
    $riffSize = [UInt32](36 + $DataBytes)
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
    try {
        $w = [System.IO.BinaryWriter]::new($stream, [System.Text.Encoding]::ASCII, $true)
        try {
            Write-Ascii $w 'RIFF'; Write-UInt32Le $w $riffSize
            Write-Ascii $w 'WAVE'; Write-Ascii $w 'fmt '
            Write-UInt32Le $w 16; Write-UInt16Le $w 1; Write-UInt16Le $w $ch
            Write-UInt32Le $w $sr; Write-UInt32Le $w $br
            Write-UInt16Le $w $ba; Write-UInt16Le $w $bps
            Write-Ascii $w 'data'; Write-UInt32Le $w $DataBytes
            $chunk = New-Object byte[] 65536; $remaining = [UInt64]$DataBytes
            while ($remaining -gt 0) {
                $c = [int][Math]::Min($chunk.Length, $remaining)
                $w.Write($chunk, 0, $c); $remaining -= [UInt64]$c
            }
        } finally { $w.Dispose() }
    } finally { $stream.Dispose() }
}

function Write-MinimalMp4 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][UInt32]$ApproxBytes
    )
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
    try {
        $w = [System.IO.BinaryWriter]::new($stream, [System.Text.Encoding]::ASCII, $true)
        try {
            $ftyp = [System.Text.Encoding]::ASCII.GetBytes('isom')
            $ftypSize = [UInt32](8 + $ftyp.Length + 4)
            $b = [BitConverter]::GetBytes([UInt32]$ftypSize); [Array]::Reverse($b); $w.Write($b)
            $w.Write([System.Text.Encoding]::ASCII.GetBytes('ftyp')); $w.Write($ftyp)
            $w.Write([BitConverter]::GetBytes([UInt32]0))
            $mdatPayload = [Math]::Max(0, $ApproxBytes - 16)
            $mdatSize = [UInt32](8 + $mdatPayload)
            $b2 = [BitConverter]::GetBytes([UInt32]$mdatSize); [Array]::Reverse($b2); $w.Write($b2)
            $w.Write([System.Text.Encoding]::ASCII.GetBytes('mdat'))
            $chunk = New-Object byte[] 65536; $remaining = [int]$mdatPayload
            while ($remaining -gt 0) {
                $c = [Math]::Min($chunk.Length, $remaining)
                $w.Write($chunk, 0, $c); $remaining -= $c
            }
        } finally { $w.Dispose() }
    } finally { $stream.Dispose() }
}

function Write-MinimalWebM {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][UInt32]$ApproxBytes
    )
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
    try {
        $w = [System.IO.BinaryWriter]::new($stream, [System.Text.Encoding]::ASCII, $true)
        try {
            $w.Write([byte[]](0x1A,0x45,0xDF,0xA3,0x83,0x42,0x86,0x81,0x01))
            $w.Write([byte[]](0x18,0x53,0x80,0x67,0x01,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF))
            $w.Write([byte[]](0x1F,0x43,0xB6,0x75,0x01,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF))
            $chunk = New-Object byte[] 65536; $remaining = [Math]::Max(0, $ApproxBytes - 36)
            while ($remaining -gt 0) {
                $c = [Math]::Min($chunk.Length, $remaining)
                $w.Write($chunk, 0, $c); $remaining -= $c
            }
        } finally { $w.Dispose() }
    } finally { $stream.Dispose() }
}

# --- main script ---

if (-not $SkipProtocolGuard) {
    & (Join-Path $repoRoot 'scripts/check-media-protocol-guard.ps1')
}

$fixtureDir = New-Item -ItemType Directory -Path (Join-Path $outputPath.FullName 'logical-media-evidence') -Force

$smallPath = Join-Path $fixtureDir.FullName 'small-inline.wav'
$largePath = Join-Path $fixtureDir.FullName 'large-protocol.wav'
$smallMp4Path = Join-Path $fixtureDir.FullName 'small-inline.mp4'
$largeMp4Path = Join-Path $fixtureDir.FullName 'large-protocol.mp4'
$smallWebmPath = Join-Path $fixtureDir.FullName 'small-inline.webm'
$largeWebmPath = Join-Path $fixtureDir.FullName 'large-protocol.webm'
$notesPath = Join-Path $outputPath.FullName 'WEBVIEW2_MEDIA_SMOKE.md'

Write-WavPcm16Mono -Path $smallPath -DataBytes (44100 * 2)
Write-WavPcm16Mono -Path $largePath -DataBytes (21 * 1024 * 1024)
Write-MinimalMp4 -Path $smallMp4Path -ApproxBytes 8192
Write-MinimalMp4 -Path $largeMp4Path -ApproxBytes (21 * 1024 * 1024)
Write-MinimalWebM -Path $smallWebmPath -ApproxBytes 8192
Write-MinimalWebM -Path $largeWebmPath -ApproxBytes (21 * 1024 * 1024)

$smallSize = (Get-Item -LiteralPath $smallPath).Length
$largeSize = (Get-Item -LiteralPath $largePath).Length
$smallMp4Size = (Get-Item -LiteralPath $smallMp4Path).Length
$largeMp4Size = (Get-Item -LiteralPath $largeMp4Path).Length
$smallWebmSize = (Get-Item -LiteralPath $smallWebmPath).Length
$largeWebmSize = (Get-Item -LiteralPath $largeWebmPath).Length

$smokeDate = Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz'
$notes = @"
# WebView2 Media Smoke Harness
Generated: $smokeDate
## Fixture
- Logical evidence directory: $($fixtureDir.FullName)
- Small inline WAV: $smallPath ($smallSize bytes)
- Large protocol WAV: $largePath ($largeSize bytes)
- Small inline MP4: $smallMp4Path ($smallMp4Size bytes)
- Large protocol MP4: $largeMp4Path ($largeMp4Size bytes)
- Small inline WebM: $smallWebmPath ($smallWebmSize bytes)
- Large protocol WebM: $largeWebmPath ($largeWebmSize bytes)
Files > 20 MiB should use evidence-media://handle/ protocol URL (mode=protocol).
## Manual WebView2 Steps
1. Build/launch desktop app
2. Create or open a case
3. Import the logical evidence directory above
4. For each format (WAV, MP4, WebM): select small inline file, verify inline preview
5. Select large protocol file, verify evidence-media://handle/ URL
6. Play and seek forward/backward in WebView2 media control
7. Confirm no host fixture path exposed in UI/debug text
8. If playback fails, verify extract file fallback works
Record results in docs/progress-ledger.md
"@

Set-Content -LiteralPath $notesPath -Value $notes -Encoding UTF8

Write-Host "Prepared WebView2 media smoke fixture:"
Write-Host "  Evidence directory: $($fixtureDir.FullName)"
Write-Host "  Small WAV: $smallPath ($smallSize bytes)"
Write-Host "  Large WAV: $largePath ($largeSize bytes)"
Write-Host "  Small MP4: $smallMp4Path ($smallMp4Size bytes)"
Write-Host "  Large MP4: $largeMp4Path ($largeMp4Size bytes)"
Write-Host "  Small WebM: $smallWebmPath ($smallWebmSize bytes)"
Write-Host "  Large WebM: $largeWebmPath ($largeWebmSize bytes)"
Write-Host "  Checklist: $notesPath"
