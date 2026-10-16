<#
.SYNOPSIS
Freeze a Cargo-built benchmark executable, source, protocol and explicit input manifests without measuring import performance.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string[]]$FixturePaths,
  [string]$OutputDir = '',
  [string]$ProtocolPath = '',
  [switch]$Release
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
foreach ($name in @('Input', 'Process', 'Protocol')) {
  Import-Module (Join-Path $PSScriptRoot "lib/ImportBenchmark.$name.psm1") -Force
}
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (@($FixturePaths | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count) { throw 'Explicit input paths must not be empty' }
foreach ($path in $FixturePaths) { $null = @(Get-ImportBenchmarkSegments -Path $path) }
if (-not $ProtocolPath) { $ProtocolPath = Join-Path $PSScriptRoot 'baselines/import-optimization-protocol.json' }
$protocol = Read-ImportBenchmarkProtocol -Path $ProtocolPath
if (-not $OutputDir) { $OutputDir = Join-Path $repo 'artifacts/import-snapshots' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputDir)) ('snapshot-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$null = New-Item -ItemType Directory -Path $directory
$code = Get-ImportBenchmarkCodeContext -RepoRoot $repo
$serializedCode = $code | ConvertTo-Json -Depth 8 -Compress
$executable = Get-ImportBenchmarkExecutable -RepoRoot $repo -Directory $directory -Release:$Release
$binaryHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
$frozen = Join-Path $directory 'import-benchmark.exe'
Copy-Item -LiteralPath $executable -Destination $frozen -ErrorAction Stop
$inputs = @($FixturePaths | ForEach-Object { Get-ImportBenchmarkManifest -Path $_ })
foreach ($inputEntry in $inputs) {
  $after = Get-ImportBenchmarkManifest -Path $inputEntry.path
  if ($after.manifestSha256 -cne $inputEntry.manifestSha256) { throw 'Input changed while capturing snapshot' }
}
if ($serializedCode -cne ((Get-ImportBenchmarkCodeContext -RepoRoot $repo) | ConvertTo-Json -Depth 8 -Compress) -or
    (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $binaryHash -or
    (Get-FileHash -LiteralPath $frozen -Algorithm SHA256).Hash.ToLowerInvariant() -cne $binaryHash -or
    (Read-ImportBenchmarkProtocol -Path $protocol.path).sha256 -cne $protocol.sha256) {
  throw 'Code, executable or protocol changed while capturing snapshot'
}
$hostInfo = [ordered]@{ os = [Environment]::OSVersion.VersionString; processors = [Environment]::ProcessorCount
  cpuName = $null; physicalMemoryBytes = $null; details = 'available' }
try {
  $hostInfo.cpuName = (Get-CimInstance Win32_Processor -ErrorAction Stop | Select-Object -First 1).Name
  $hostInfo.physicalMemoryBytes = (Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).TotalPhysicalMemory
} catch { $hostInfo.details = 'unavailable' }
$manifest = [ordered]@{
  schemaVersion = 1; capturedUtc = [DateTime]::UtcNow.ToString('o')
  kind = 'snapshot-only-no-performance-verdict'; codeContext = $code
  worktreeStatus = @(& git -C $repo status --porcelain)
  executable = [ordered]@{ path = $frozen; sha256 = $binaryHash; buildProfile = if ($Release) { 'release' } else { 'test' } }
  protocol = $protocol; inputs = $inputs
  hostInfo = $hostInfo
  buildEnvironment = [ordered]@{ rustflags = $env:RUSTFLAGS; cargoBuildTarget = $env:CARGO_BUILD_TARGET
    cargoIncremental = $env:CARGO_INCREMENTAL; rayonNumThreads = $env:RAYON_NUM_THREADS }
  toolchain = [ordered]@{ rustc = @(& rustc -Vv); cargo = @(& cargo -V); powershell = $PSVersionTable.PSVersion.ToString() }
  integrityVerified = $true; performanceVerdict = 'not-measured'
}
[IO.File]::WriteAllText((Join-Path $directory 'run-manifest.json'), ($manifest | ConvertTo-Json -Depth 30), (New-Object Text.UTF8Encoding($false)))
Write-Host "Frozen benchmark snapshot: $directory"
