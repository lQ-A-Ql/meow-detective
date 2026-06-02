param(
    [string]$OutputDir = (Join-Path ([System.IO.Path]::GetTempPath()) 'forensics-webview2-media-smoke'),
    [switch]$SkipProtocolGuard
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$outputPath = New-Item -ItemType Directory -Path $OutputDir -Force

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

    if (($DataBytes % 2) -ne 0) {
        throw 'DataBytes must be even for PCM16 mono samples'
    }

    $sampleRate = [UInt32]44100
    $bitsPerSample = [UInt16]16
    $channels = [UInt16]1
    $blockAlign = [UInt16](($channels * $bitsPerSample) / 8)
    $byteRate = [UInt32]($sampleRate * $blockAlign)
    $riffSize = [UInt32](36 + $DataBytes)

    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
    try {
        $writer = [System.IO.BinaryWriter]::new($stream, [System.Text.Encoding]::ASCII, $true)
        try {
            Write-Ascii $writer 'RIFF'
            Write-UInt32Le $writer $riffSize
            Write-Ascii $writer 'WAVE'
            Write-Ascii $writer 'fmt '
            Write-UInt32Le $writer 16
            Write-UInt16Le $writer 1
            Write-UInt16Le $writer $channels
            Write-UInt32Le $writer $sampleRate
            Write-UInt32Le $writer $byteRate
            Write-UInt16Le $writer $blockAlign
            Write-UInt16Le $writer $bitsPerSample
            Write-Ascii $writer 'data'
            Write-UInt32Le $writer $DataBytes

            $chunk = New-Object byte[] 65536
            $remaining = [UInt64]$DataBytes
            while ($remaining -gt 0) {
                $count = [int][Math]::Min($chunk.Length, $remaining)
                $writer.Write($chunk, 0, $count)
                $remaining -= [UInt64]$count
            }
        } finally {
            $writer.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

if (-not $SkipProtocolGuard) {
    & (Join-Path $repoRoot 'scripts/check-media-protocol-guard.ps1')
}

$fixtureDir = New-Item -ItemType Directory -Path (Join-Path $outputPath.FullName 'logical-media-evidence') -Force
$smallPath = Join-Path $fixtureDir.FullName 'small-inline.wav'
$largePath = Join-Path $fixtureDir.FullName 'large-protocol.wav'
$notesPath = Join-Path $outputPath.FullName 'WEBVIEW2_MEDIA_SMOKE.md'

Write-WavPcm16Mono -Path $smallPath -DataBytes (44100 * 2)
Write-WavPcm16Mono -Path $largePath -DataBytes (21 * 1024 * 1024)

$smallSize = (Get-Item -LiteralPath $smallPath).Length
$largeSize = (Get-Item -LiteralPath $largePath).Length

$notes = @"
# WebView2 Media Smoke Harness

Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')

## Fixture

- Logical evidence directory: $($fixtureDir.FullName)
- Small inline media: $smallPath ($smallSize bytes)
- Large protocol media: $largePath ($largeSize bytes)

`large-protocol.wav` is intentionally larger than the 20 MiB inline media limit,
so FileBrowser should request `mode=protocol` and render an `evidence-media://`
URL instead of a data URL or host filesystem path.

## Manual WebView2 Steps

1. Build or launch the desktop app:
   - `pnpm --dir frontend build`
   - `cargo tauri dev` from `apps/desktop/src-tauri`, or launch the built app.
2. Create or open a case.
3. Import the logical evidence directory listed above.
4. Open Files, select `small-inline.wav`, and verify the preview uses an inline
   data URL.
5. Select `large-protocol.wav`, open Preview, and verify the UI shows controlled
   streaming preview text.
6. Play the audio and seek forward/backward in the WebView2 media control.
7. Confirm the rendered media source starts with `evidence-media://handle/`.
8. Confirm the UI/debug text does not expose the fixture host path.
9. If playback or seek fails, verify that the fallback extraction flow still
   works through the FileBrowser "extract file" action.

Record the result in `docs/开发记录.md` and
`development-reports/sessions/2026-06-02.md`. This harness prepares evidence and
guard checks; it does not replace the human WebView2 playback/seek result.
"@

Set-Content -LiteralPath $notesPath -Value $notes -Encoding UTF8

Write-Host "Prepared WebView2 media smoke fixture:"
Write-Host "  Evidence directory: $($fixtureDir.FullName)"
Write-Host "  Small media: $smallPath ($smallSize bytes)"
Write-Host "  Large media: $largePath ($largeSize bytes)"
Write-Host "  Checklist: $notesPath"
