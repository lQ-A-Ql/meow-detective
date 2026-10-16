Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-ImportBenchmarkSegments {
  param([string]$Path)
  $file = Get-Item -LiteralPath $Path -ErrorAction Stop
  if ($file.PSIsContainer) { throw 'Fixture must be a regular RAW/dd/img or E01 file' }
  if ($file.Extension -match '^\.(raw|dd|img)$') { return $file }
  if ($file.Extension -ine '.E01') { throw 'Benchmark accepts single-file RAW/dd/img or the first E01 segment' }
  # Match image-e01::build_segment_path: E01, E02, ... E99, E100, ...
  # Refuse gaps or noncanonical numbering instead of hashing a partial set.
  $pattern = '^' + [regex]::Escape($file.BaseName) + '\.E([0-9]{2,})$'
  $segments = @(Get-ChildItem -LiteralPath $file.DirectoryName -File | Where-Object { $_.Name -match $pattern } |
    Sort-Object { [long]([regex]::Match($_.Name, $pattern, 'IgnoreCase').Groups[1].Value) })
  for ($index = 0; $index -lt $segments.Count; $index++) {
    $expected = '{0}.E{1:00}' -f $file.BaseName, ($index + 1)
    if ($segments[$index].Name -ine $expected) { throw "Noncontiguous E01 set; expected $expected" }
  }
  $segments
}

function Get-ImportBenchmarkManifest {
  param([string]$Path)
  $segments = @(Get-ImportBenchmarkSegments -Path $Path)
  $manifest = @(foreach ($file in $segments) {
    $length = $file.Length
    $modified = $file.LastWriteTimeUtc.Ticks
    $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $file.Refresh()
    if (-not $file.Exists -or $file.Length -ne $length -or $file.LastWriteTimeUtc.Ticks -ne $modified) {
      throw 'Input changed while hashing'
    }
    [ordered]@{ name = $file.Name; bytes = $length; sha256 = $hash; modifiedUtcTicks = $modified }
  })
  $canonical = $manifest | ConvertTo-Json -Depth 5 -Compress
  $sha = [Security.Cryptography.SHA256]::Create()
  try { $digest = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($canonical))).Replace('-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
  [pscustomobject]@{
    path = (Resolve-Path -LiteralPath $Path).Path
    format = if ([IO.Path]::GetExtension($Path) -ieq '.E01') { 'e01' } else { 'raw' }
    manifestSha256 = $digest; segments = $manifest
  }
}

function Get-ImportBenchmarkCodeContext {
  param([string]$RepoRoot)
  $revision = @(& git -C $RepoRoot rev-parse HEAD)
  if ($LASTEXITCODE -ne 0 -or $revision.Count -ne 1) { throw 'Cannot resolve Git revision' }
  $files = @(& git -c core.quotepath=false -C $RepoRoot ls-files --cached --others --exclude-standard)
  if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate code inputs' }
  # Capture clean files too: a commit during the run must not hide changed input.
  $code = @($files | Where-Object { $_ -match '(\.(rs|sql|ps1|psm1|toml)$|Cargo\.lock$|\.cargo/)' -or
      $_ -match '^scripts/baselines/import-optimization-[^/]+\.json$' } |
    Sort-Object -Unique | ForEach-Object {
      $path = Join-Path $RepoRoot $_
      [ordered]@{ path = $_; sha256 = if (Test-Path -LiteralPath $path -PathType Leaf) {
        (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
      } else { $null } }
    })
  [ordered]@{ revision = $revision[0].Trim(); files = $code }
}

Export-ModuleMember -Function Get-ImportBenchmarkSegments, Get-ImportBenchmarkManifest, Get-ImportBenchmarkCodeContext
