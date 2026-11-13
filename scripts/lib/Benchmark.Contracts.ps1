. (Join-Path $PSScriptRoot 'Benchmark.Workload.ps1')

function Get-BenchmarkCatalog {
  param([string]$Repository)
  $path = Join-Path $Repository 'scripts/baselines/performance-benchmark-scenarios.json'
  $catalog = Get-Content -LiteralPath $path -Raw -Encoding UTF8 | ConvertFrom-Json
  if ($catalog.schemaVersion -ne 1) { throw 'Unsupported benchmark catalog version' }
  if ([string]::IsNullOrWhiteSpace($catalog.harnessVersion)) { throw 'Benchmark harness version is required' }
  $names = @($catalog.scenarioNames)
  if ($names.Count -ne @($names | Sort-Object -Unique).Count) {
    throw 'Benchmark scenario names must be unique'
  }
  foreach ($name in $catalog.smoke) {
    if ($name -notin $names) { throw "Unknown smoke scenario: $name" }
  }
  return $catalog
}

function Select-BenchmarkScenarios {
  param($Catalog, [string]$Requested)
  if ($Requested -eq 'all') { return @($Catalog.scenarioNames) }
  if ($Requested -eq 'smoke') { return @($Catalog.smoke) }
  if ($Requested -notin $Catalog.scenarioNames) { throw "Unknown benchmark scenario: $Requested" }
  return @($Requested)
}

function Assert-BenchmarkScenario {
  param($Output, [string]$Name, [int]$Runs, [int]$WarmupRuns, [string]$FixtureLevel, [string]$BuildProfile, $Catalog)
  if ($Output.schemaVersion -ne 2) { throw 'Unsupported benchmark output version' }
  if ($Output.benchmarkVersion -cne $Catalog.harnessVersion) { throw 'Benchmark harness version differs from catalog' }
  if ($Output.buildProfile -ne $BuildProfile) { throw 'Benchmark build profile differs from request' }
  if (@($Output.scenarios).Count -ne 1) { throw 'An isolated benchmark must emit exactly one scenario' }
  $scenario = $Output.scenarios[0]
  if ($scenario.scenario -ne $Name -or $scenario.datasetLevel -ne $FixtureLevel) {
    throw 'Benchmark returned an unexpected scenario or fixture level'
  }
  if ($scenario.warmupRuns -ne $WarmupRuns -or @($scenario.runs).Count -ne $Runs) {
    throw "Benchmark sample count differs from request: $Name"
  }
  $requiredCounters = @('pointReads', 'scanEntries', 'fjallDatabaseOpens', 'sourceSessionOpens',
    'searchIndexOpens', 'querySessionOpens', 'rankedHits', 'mergeComparisons',
    'bytesRead', 'bytesWritten', 'syncAllCount', 'pathNormalizations')
  foreach ($sample in $scenario.runs) {
    if ($sample.elapsedNs -lt 0 -or $sample.elapsedMs -lt 0) { throw 'Negative benchmark duration' }
    if ([long][Math]::Floor($sample.elapsedNs / 1000000) -ne $sample.elapsedMs) {
      throw "Inconsistent duration units in $Name"
    }
    foreach ($counter in $requiredCounters) {
      if ($counter -notin $sample.counters.PSObject.Properties.Name -or $null -eq $sample.counters.$counter) {
        throw "Missing per-run counter $counter in $Name"
      }
    }
    if ($sample.memory.sampledPeakRssMb -le 0 -or
        $sample.memory.sampledPeakRssMb -lt $sample.memory.rssAtStartMb -or
        $sample.memory.sampledPeakRssMb -lt $sample.memory.rssAtEndMb) {
      throw "Invalid sampled peak RSS in $Name"
    }
  }
  $ordered = @($scenario.runs | ForEach-Object { [long]$_.elapsedMs } | Sort-Object)
  $expected = $ordered[[int][Math]::Ceiling($ordered.Count * 0.95) - 1]
  if ($scenario.p95Ms -ne $expected) { throw "Incorrect p95 for $Name" }
  $peak = ($scenario.runs | ForEach-Object { $_.memory.sampledPeakRssMb } | Measure-Object -Maximum).Maximum
  if ($scenario.memoryPeakMb -ne $peak) { throw "Incorrect peak RSS aggregate for $Name" }
  Assert-BenchmarkWorkload -Scenario $scenario -Catalog $Catalog -Level $FixtureLevel
}

function Get-BenchmarkHostMetadata {
  param([string]$Repository)
  $os = Get-CimInstance Win32_OperatingSystem
  $cpu = @(Get-CimInstance Win32_Processor | Select-Object -ExpandProperty Name)
  $disks = @(Get-CimInstance Win32_DiskDrive | Select-Object Model, InterfaceType, Size)
  Push-Location -LiteralPath $Repository
  try {
    $rust = (& rustc -Vv) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw 'Unable to identify Rust compiler' }
  } finally { Pop-Location }
  $revision = (& git -C $Repository rev-parse HEAD).Trim()
  if ($LASTEXITCODE -ne 0) { throw 'Unable to identify source revision' }
  $diff = (& git -C $Repository diff HEAD --binary) -join "`n"
  if ($LASTEXITCODE -ne 0) { throw 'Unable to describe worktree changes' }
  $hash = [System.Security.Cryptography.SHA256]::Create()
  try {
    $digest = [BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($diff))).Replace('-', '').ToLowerInvariant()
  } finally { $hash.Dispose() }
  $untracked = @(& git -C $Repository ls-files --others --exclude-standard)
  if ($LASTEXITCODE -ne 0) { throw 'Unable to enumerate untracked source files' }
  $untrackedHashes = @($untracked | ForEach-Object {
    $path = Join-Path $Repository $_
    [ordered]@{ path = $_; sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() }
  })
  return [ordered]@{
    display = "$($os.Caption) / $([Math]::Round($os.TotalVisibleMemorySize / 1MB, 2)) GiB RAM"
    operatingSystem = $os.Caption
    architecture = $os.OSArchitecture
    ramBytes = [long]$os.TotalVisibleMemorySize * 1024
    processors = $cpu
    disks = $disks
    rustc = $rust
    sourceRevision = $revision
    worktreeDiffSha256 = $digest
    untrackedSources = $untrackedHashes
  }
}
