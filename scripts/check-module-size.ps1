# Requires -Version 5.1
<#
.SYNOPSIS
  CI guard: lock backend Rust module-size debt during the module refactor.
.DESCRIPTION
  Scans production Rust source under each crate src directory and
  apps/desktop/src-tauri/src, plus each owning unit's build.rs. Normal modules
  have a 500-line target and an 800-line hard ceiling for new debt; mod.rs and
  lib.rs have a 200-line ceiling. Existing migration debt is identity-locked
  by a reference-revision transition and may only shrink or be removed.

  Use -GenerateBaseline to print the current CSV to stdout. Use -SelfTest to
  exercise transition, boundary, exclusion, and stale-baseline policy without
  writing repository files.
#>
param(
  [string]$BaselinePath,
  [string]$ExceptionPath,
  [string]$BootstrapManifestPath,
  [string]$TrustedBootstrapSha256,
  [string]$ReferenceRevision,
  [switch]$GenerateBaseline,
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'lib/RustGuard.Common.ps1')
if ([string]::IsNullOrWhiteSpace($BaselinePath)) {
  $BaselinePath = Join-Path $repoRoot 'scripts/baselines/rust-module-size-baseline.csv'
}
if ([string]::IsNullOrWhiteSpace($ExceptionPath)) {
  $ExceptionPath = Join-Path $repoRoot 'scripts/baselines/rust-module-size-exceptions.csv'
}
if ([string]::IsNullOrWhiteSpace($BootstrapManifestPath)) {
  $BootstrapManifestPath = Join-Path $repoRoot 'scripts/baselines/rust-module-size-bootstrap.csv'
}
if ([string]::IsNullOrWhiteSpace($TrustedBootstrapSha256)) {
  $TrustedBootstrapSha256 = $env:RUST_MODULE_SIZE_BOOTSTRAP_SHA256
}

$TargetLines = 500
$HardLines = 800
$ModuleRootHardLines = 200
$InitialBootstrapReference = '2087df1cc5209fa879cdb3796e9a1437196bc2f4'
$StrictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)

function Read-StrictUtf8Text {
  param([Parameter(Mandatory = $true)][string]$Path)

  try {
    return $StrictUtf8.GetString([System.IO.File]::ReadAllBytes($Path))
  } catch {
    throw "File is not valid UTF-8: $Path"
  }
}

function Get-NormalizedRelativePath {
  param([Parameter(Mandatory = $true)][string]$FullName)

  return Get-RustGuardRepositoryRelativePath -RepoRoot $repoRoot -FullName $FullName
}

function Test-IsNormalizedRepositoryPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  return Test-RustGuardNormalizedRepositoryPath -Path $Path
}

function Get-LineCountFromText {
  param([Parameter(Mandatory = $true)][string]$Content)

  if ($Content.Length -eq 0) {
    return 0
  }

  $lineFeeds = ([regex]::Matches($Content, "`n")).Count
  if ($Content.EndsWith("`n")) {
    return $lineFeeds
  }

  return $lineFeeds + 1
}

function Get-OrdinalSortedStrings {
  param([Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Values)

  return Get-RustGuardOrdinalSortedStrings -Values $Values
}

function Test-IsExcludedRustRepositoryPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  return -not (Test-RustGuardProductionRepositoryPath -Path $Path)
}

function Test-IsAllowedProductionRustRepositoryPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  if (-not (Test-IsNormalizedRepositoryPath -Path $Path)) {
    return $false
  }
  if (Test-IsExcludedRustRepositoryPath -Path $Path) {
    return $false
  }

  return Test-RustGuardProductionRepositoryPath -Path $Path
}

function Get-ProductionRustFiles {
  foreach ($entry in @(Get-RustGuardFiles -RepoRoot $repoRoot -Mode Production)) {
    Write-Output $entry.File
  }
}

function Get-ModuleMetadataForRepositoryPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  $name = [System.IO.Path]::GetFileName($Path).ToLowerInvariant()
  if ($name -eq 'mod.rs' -or $name -eq 'lib.rs') {
    return [PSCustomObject]@{
      limit = $ModuleRootHardLines
      kind = 'module-root'
    }
  }
  if ($name -eq 'build.rs') {
    return [PSCustomObject]@{
      limit = $TargetLines
      kind = 'build-script'
    }
  }

  return [PSCustomObject]@{
    limit = $TargetLines
    kind = 'production'
  }
}

function Test-IsModuleViolation {
  param(
    [Parameter(Mandatory = $true)][int]$Lines,
    [Parameter(Mandatory = $true)]$Metadata
  )

  return $Lines -gt [int]$Metadata.limit
}

function Get-CurrentModuleViolations {
  param([Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Files)

  $rowsByPath = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  foreach ($file in $Files) {
    $relative = Get-NormalizedRelativePath -FullName $file.FullName
    $content = Read-StrictUtf8Text -Path $file.FullName
    $lines = Get-LineCountFromText -Content $content
    $metadata = Get-ModuleMetadataForRepositoryPath -Path $relative
    if (-not (Test-IsModuleViolation -Lines $lines -Metadata $metadata)) {
      continue
    }

    $rowsByPath[$relative] = [PSCustomObject]@{
      path = $relative
      lines = $lines
      limit = [int]$metadata.limit
      kind = [string]$metadata.kind
    }
  }

  foreach ($path in (Get-OrdinalSortedStrings -Values ([string[]]@($rowsByPath.Keys)))) {
    Write-Output $rowsByPath[$path]
  }
}

function ConvertTo-RequiredPositiveInt {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string]$Field,
    [Parameter(Mandatory = $true)][string]$Identity,
    [Parameter(Mandatory = $true)][string]$Source
  )

  return ConvertTo-RustGuardCanonicalInt -Value $Value -Field $Field -Identity "$Source::$Identity"
}

function ConvertFrom-ModuleBaselineText {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Source
  )

  $rows = @(ConvertFrom-RustGuardCsv -Content $Content -Header 'path,lines,limit,kind' -Source $Source)
  $byPath = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $previousPath = $null
  foreach ($entry in $rows) {
    foreach ($field in @('path', 'lines', 'limit', 'kind')) {
      if ([string]::IsNullOrWhiteSpace([string]$entry.$field)) {
        throw "Module-size baseline at $Source contains an empty required field '$field'"
      }
    }

    $path = [string]$entry.path
    if (-not (Test-IsNormalizedRepositoryPath -Path $path)) {
      throw "Module-size baseline path at $Source must be normalized and repository-relative: $path"
    }
    if (-not (Test-IsAllowedProductionRustRepositoryPath -Path $path)) {
      throw "Excluded or non-production source must not appear in the module-size baseline at $Source`: $path"
    }
    if ($byPath.ContainsKey($path)) {
      throw "Module-size baseline at $Source contains a duplicate path: $path"
    }
    if ($null -ne $previousPath -and
        [string]::CompareOrdinal([string]$previousPath, $path) -ge 0) {
      throw "Module-size baseline at $Source is not in deterministic ordinal path order near: $path"
    }

    $lines = ConvertTo-RequiredPositiveInt -Value $entry.lines -Field 'lines' -Identity $path -Source $Source
    $limit = ConvertTo-RequiredPositiveInt -Value $entry.limit -Field 'limit' -Identity $path -Source $Source
    $metadata = Get-ModuleMetadataForRepositoryPath -Path $path
    if ([string]$entry.kind -notin @('production', 'module-root', 'build-script')) {
      throw "Module-size baseline at $Source contains an invalid kind for $path`: $($entry.kind)"
    }
    if ($limit -ne [int]$metadata.limit -or [string]$entry.kind -ne [string]$metadata.kind) {
      throw "Module-size baseline metadata at $Source does not match $path`: expected limit=$($metadata.limit),kind=$($metadata.kind)"
    }
    if ($lines -le $limit) {
      throw "Module-size baseline at $Source must describe a violation: $path has lines=$lines and limit=$limit"
    }

    $entry.path = $path
    $entry.lines = $lines
    $entry.limit = $limit
    $entry.kind = [string]$entry.kind
    $byPath.Add($path, $entry)
    $previousPath = $path
  }

  $canonicalLines = @('path,lines,limit,kind')
  foreach ($entry in $rows) {
    $canonicalLines += ('{0},{1},{2},{3}' -f
      (Format-CsvField -Value ([string]$entry.path)),
      [int]$entry.lines,
      [int]$entry.limit,
      (Format-CsvField -Value ([string]$entry.kind)))
  }
  Assert-CanonicalCsvText `
    -Content $Content `
    -Canonical ($canonicalLines -join "`n") `
    -Source $Source

  return [PSCustomObject]@{
    Rows = $rows
    ByPath = $byPath
  }
}

function Format-CsvField {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)

  return Format-RustGuardCsvField -Value $Value
}

function Assert-CanonicalCsvText {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Canonical,
    [Parameter(Mandatory = $true)][string]$Source
  )

  Assert-RustGuardCanonicalCsvText -Content $Content -Canonical $Canonical -Source $Source
}

function Write-ModuleBaselineCsv {
  param([Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Rows)

  Write-Output 'path,lines,limit,kind'
  foreach ($row in $Rows) {
    Write-Output ('{0},{1},{2},{3}' -f
      (Format-CsvField -Value ([string]$row.path)),
      [int]$row.lines,
      [int]$row.limit,
      (Format-CsvField -Value ([string]$row.kind)))
  }
}

function Get-CurrentBaselineFailures {
  param(
    [Parameter(Mandatory = $true)]$BaselineDocument,
    [Parameter(Mandatory = $true)]$ProductionFilesByPath,
    [Parameter(Mandatory = $true)]$CurrentByPath
  )

  $failures = @()
  foreach ($entry in $BaselineDocument.Rows) {
    $path = [string]$entry.path
    if (-not $ProductionFilesByPath.ContainsKey($path)) {
      $failures += "stale baseline path is not a current production Rust file: $path"
      continue
    }
    if (-not $CurrentByPath.ContainsKey($path)) {
      $failures += "stale resolved baseline path no longer exceeds its limit: $path"
      continue
    }

    $current = $CurrentByPath[$path]
    if ([int]$entry.limit -ne [int]$current.limit -or
        [string]$entry.kind -ne [string]$current.kind) {
      $failures += "baseline metadata drifted from the current file for $path"
    }
  }

  return $failures
}

function Get-RepositoryRelativeFilePath {
  param([Parameter(Mandatory = $true)][string]$Path)

  return Get-NormalizedRelativePath -FullName ([System.IO.Path]::GetFullPath($Path))
}

function Invoke-GitCapture {
  param([Parameter(Mandatory = $true)][string[]]$Arguments)

  $previousPreference = $ErrorActionPreference
  try {
    $ErrorActionPreference = 'Continue'
    $output = @(& git @Arguments 2>&1)
    $exitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $previousPreference
  }

  return [PSCustomObject]@{
    ExitCode = $exitCode
    Output = @($output | ForEach-Object { [string]$_ })
  }
}

function Resolve-ReferenceCommit {
  $candidate = $ReferenceRevision
  if ([string]::IsNullOrWhiteSpace($candidate)) {
    $candidate = [Environment]::GetEnvironmentVariable('RUST_MODULE_SIZE_BASELINE_REFERENCE')
  }
  if ([string]::IsNullOrWhiteSpace($candidate)) {
    $candidate = 'HEAD'
  }

  $result = Invoke-GitCapture -Arguments @(
    '-C',
    $repoRoot,
    'rev-parse',
    '--verify',
    "$candidate^{commit}"
  )
  if ($result.ExitCode -ne 0 -or
      $result.Output.Count -ne 1 -or
      $result.Output[0] -notmatch '^[0-9a-fA-F]{40}$') {
    throw "Unable to resolve module-size baseline reference revision '$candidate': $($result.Output -join ' ')"
  }

  return ([string]$result.Output[0]).ToLowerInvariant()
}

function Get-GitFileAtRevision {
  param(
    [Parameter(Mandatory = $true)][string]$Revision,
    [Parameter(Mandatory = $true)][string]$RepositoryPath
  )

  $object = '{0}:{1}' -f $Revision, $RepositoryPath
  $existsResult = Invoke-GitCapture -Arguments @('-C', $repoRoot, 'cat-file', '-e', $object)
  if ($existsResult.ExitCode -ne 0) {
    return [PSCustomObject]@{
      Exists = $false
      Content = $null
    }
  }

  $showResult = Invoke-GitCapture -Arguments @('-C', $repoRoot, 'show', $object)
  if ($showResult.ExitCode -ne 0) {
    throw "Unable to read $RepositoryPath from reference revision $Revision`: $($showResult.Output -join ' ')"
  }

  return [PSCustomObject]@{
    Exists = $true
    Content = ($showResult.Output -join [Environment]::NewLine)
  }
}

function Get-Sha256Hex {
  param([Parameter(Mandatory = $true)][byte[]]$Bytes)

  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $digest = $sha.ComputeHash($Bytes)
  } finally {
    $sha.Dispose()
  }

  return (($digest | ForEach-Object { $_.ToString('x2') }) -join '')
}

function Assert-BootstrapManifest {
  param(
    [Parameter(Mandatory = $true)][string]$ReferenceCommit,
    [AllowEmptyString()][string]$ExpectedSha256 = $TrustedBootstrapSha256,
    [string]$ManifestPath = $BootstrapManifestPath,
    [string]$BaselineFilePath = $BaselinePath
  )

  if ($ReferenceCommit -ne $InitialBootstrapReference) {
    throw "Module-size bootstrap is only authorized against $InitialBootstrapReference, not $ReferenceCommit"
  }
  if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "Module-size baseline is absent at reference $ReferenceCommit and requires bootstrap manifest: $ManifestPath"
  }

  $content = Read-StrictUtf8Text -Path $ManifestPath
  $firstLine = @($content -split '\r?\n')[0]
  if ($firstLine -ne 'referenceRevision,baselineSha256') {
    throw 'Module-size bootstrap manifest must use the exact header: referenceRevision,baselineSha256'
  }
  $rows = @($content | ConvertFrom-Csv)
  if ($rows.Count -ne 1) {
    throw 'Module-size bootstrap manifest must contain exactly one authorization row'
  }

  $row = $rows[0]
  if ([string]$row.referenceRevision -notmatch '^[0-9a-f]{40}$' -or
      [string]$row.referenceRevision -ne $InitialBootstrapReference -or
      [string]$row.referenceRevision -ne $ReferenceCommit) {
    throw "Module-size bootstrap manifest must authorize exactly $InitialBootstrapReference"
  }
  if ([string]$row.baselineSha256 -notmatch '^[0-9a-f]{64}$') {
    throw 'Module-size bootstrap manifest contains an invalid baselineSha256'
  }

  $canonical = 'referenceRevision,baselineSha256{0}{1},{2}' -f
    "`n",
    [string]$row.referenceRevision,
    [string]$row.baselineSha256
  Assert-CanonicalCsvText -Content $content -Canonical $canonical -Source $ManifestPath

  $actualHash = Get-Sha256Hex -Bytes ([System.IO.File]::ReadAllBytes($BaselineFilePath))
  if ($actualHash -ne [string]$row.baselineSha256) {
    throw "Module-size bootstrap does not authorize the current baseline bytes: expected $($row.baselineSha256), found $actualHash"
  }
  Assert-RustGuardTrustedBootstrapSha256 `
    -GuardName 'Module-size' `
    -ExpectedSha256 $ExpectedSha256 `
    -ManifestSha256 ([string]$row.baselineSha256) `
    -ActualSha256 $actualHash
}

function Get-BaselineTransitionFailures {
  param(
    [Parameter(Mandatory = $true)]$CurrentBaseline,
    [Parameter(Mandatory = $true)]$ReferenceBaseline
  )

  $failures = @()
  foreach ($entry in $CurrentBaseline.Rows) {
    $path = [string]$entry.path
    if (-not $ReferenceBaseline.ByPath.ContainsKey($path)) {
      $failures += "baseline transition added path: $path"
      continue
    }

    $reference = $ReferenceBaseline.ByPath[$path]
    if ([int]$entry.lines -gt [int]$reference.lines) {
      $failures += "baseline transition increased allowance for $path from $($reference.lines) to $($entry.lines)"
    }
    if ([int]$entry.limit -ne [int]$reference.limit -or
        [string]$entry.kind -ne [string]$reference.kind) {
      $failures += "baseline transition changed limit/kind metadata for $path"
    }
  }

  return $failures
}

function Assert-BaselineTransition {
  param(
    [Parameter(Mandatory = $true)]$CurrentBaseline,
    [Parameter(Mandatory = $true)][string]$ReferenceCommit
  )

  $baselineRepoPath = Get-RepositoryRelativeFilePath -Path $BaselinePath
  $referenceFile = Get-GitFileAtRevision -Revision $ReferenceCommit -RepositoryPath $baselineRepoPath
  if (-not $referenceFile.Exists) {
    Assert-BootstrapManifest -ReferenceCommit $ReferenceCommit
    Write-Host "Rust module baseline transition: one-time bootstrap authorized against $ReferenceCommit"
    return
  }

  $source = '{0}:{1}' -f $ReferenceCommit, $baselineRepoPath
  $referenceBaseline = ConvertFrom-ModuleBaselineText -Content $referenceFile.Content -Source $source
  $failures = @(
    Get-BaselineTransitionFailures -CurrentBaseline $CurrentBaseline -ReferenceBaseline $referenceBaseline
  )
  if ($failures.Count -gt 0) {
    throw ("Module-size baseline transition rejected against {0}:{1}{2}" -f
      $ReferenceCommit,
      [Environment]::NewLine,
      ($failures -join [Environment]::NewLine))
  }

  Write-Host ('Rust module baseline transition passed: reference={0}, reference rows={1}, current rows={2}; only decreases/deletions allowed' -f
    $ReferenceCommit,
    $referenceBaseline.Rows.Count,
    $CurrentBaseline.Rows.Count)
}

function ConvertFrom-ModuleExceptionsText {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)]$ProductionFilesByPath,
    [Parameter(Mandatory = $true)]$CurrentByPath,
    [Parameter(Mandatory = $true)]$BaselineByPath
  )

  $firstLine = @($Content -split '\r?\n')[0]
  if ($firstLine -ne 'path,owner,reason,expires') {
    throw "Module-size exception list at $Source must use the exact header: path,owner,reason,expires"
  }

  $rows = @($Content | ConvertFrom-Csv)
  $byPath = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $previousPath = $null
  $today = [DateTime]::UtcNow.Date
  foreach ($entry in $rows) {
    foreach ($field in @('path', 'owner', 'reason', 'expires')) {
      if ([string]::IsNullOrWhiteSpace([string]$entry.$field)) {
        throw "Module-size exception at $Source contains an empty required field '$field'"
      }
    }

    $path = [string]$entry.path
    if (-not (Test-IsNormalizedRepositoryPath -Path $path)) {
      throw "Module-size exception path at $Source must be normalized and repository-relative: $path"
    }
    if ($byPath.ContainsKey($path)) {
      throw "Module-size exception at $Source contains a duplicate path: $path"
    }
    if ($null -ne $previousPath -and
        [string]::CompareOrdinal([string]$previousPath, $path) -ge 0) {
      throw "Module-size exception list at $Source is not in deterministic ordinal path order near: $path"
    }
    if (-not $ProductionFilesByPath.ContainsKey($path)) {
      throw "Module-size exception path is not a current production Rust file: $path"
    }
    if ($BaselineByPath.ContainsKey($path)) {
      throw "Migration baseline entries cannot also be formal exceptions: $path"
    }
    if (-not $CurrentByPath.ContainsKey($path)) {
      throw "Module-size exception is stale because the file no longer exceeds its target: $path"
    }

    $expires = [DateTime]::MinValue
    $parsed = [DateTime]::TryParseExact(
      [string]$entry.expires,
      'yyyy-MM-dd',
      [System.Globalization.CultureInfo]::InvariantCulture,
      [System.Globalization.DateTimeStyles]::None,
      [ref]$expires
    )
    if (-not $parsed) {
      throw "Module-size exception has an invalid expires date: $path=$($entry.expires)"
    }
    if ($expires.Date -lt $today) {
      throw "Module-size exception has expired: $path=$($entry.expires)"
    }

    $current = $CurrentByPath[$path]
    if ([string]$current.kind -ne 'production' -or
        [int]$current.lines -le $TargetLines -or
        [int]$current.lines -gt $HardLines) {
      throw "Module-size exception is only valid for normal 501-800 line production modules: $path has $($current.lines) lines and kind=$($current.kind)"
    }

    $byPath.Add($path, $entry)
    $previousPath = $path
  }

  $canonicalLines = @('path,owner,reason,expires')
  foreach ($entry in $rows) {
    $canonicalLines += ('{0},{1},{2},{3}' -f
      (Format-CsvField -Value ([string]$entry.path)),
      (Format-CsvField -Value ([string]$entry.owner)),
      (Format-CsvField -Value ([string]$entry.reason)),
      (Format-CsvField -Value ([string]$entry.expires)))
  }
  Assert-CanonicalCsvText `
    -Content $Content `
    -Canonical ($canonicalLines -join "`n") `
    -Source $Source

  return [PSCustomObject]@{
    Rows = $rows
    ByPath = $byPath
  }
}

function Get-ModulePolicyFailures {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$CurrentRows,
    [Parameter(Mandatory = $true)]$BaselineByPath,
    [Parameter(Mandatory = $true)]$ExceptionsByPath
  )

  $failures = @()
  foreach ($row in $CurrentRows) {
    $path = [string]$row.path
    if ($BaselineByPath.ContainsKey($path)) {
      $baselineLines = [int]$BaselineByPath[$path].lines
      if ([int]$row.lines -gt $baselineLines) {
        $failures += "increased violation: $path grew from $baselineLines to $($row.lines) lines"
      }
      continue
    }

    if ([string]$row.kind -eq 'production' -and
        [int]$row.lines -le $HardLines -and
        $ExceptionsByPath.ContainsKey($path)) {
      continue
    }

    $ceiling = $HardLines
    if ([string]$row.kind -eq 'module-root') {
      $ceiling = $ModuleRootHardLines
    }
    $failures += "new violation without a valid formal exception: $path has $($row.lines) lines (limit $($row.limit), hard ceiling $ceiling)"
  }

  return $failures
}

function Assert-SelfTestThrows {
  param(
    [Parameter(Mandatory = $true)][scriptblock]$Action,
    [Parameter(Mandatory = $true)][string]$Name
  )

  $threw = $false
  try {
    & $Action
  } catch {
    $threw = $true
  }
  if (-not $threw) {
    throw "Module-size self-test expected failure: $Name"
  }
}

function Invoke-ModuleGuardSelfTest {
  Invoke-RustGuardWorkspaceDiscoverySelfTest -Encoding $StrictUtf8 -CodeTargetAssertion {
    param(
      [System.IO.FileInfo]$TargetFile,
      [string]$TargetSource,
      [System.IO.FileInfo]$HelperFile,
      [string]$HelperSource
    )

    $moduleTargetLineTotal = ([regex]::Matches($TargetSource, "`n")).Count
    if (-not $TargetSource.EndsWith("`n")) {
      $moduleTargetLineTotal++
    }
    $recursiveHelperLineTotal = ([regex]::Matches($HelperSource, "`n")).Count
    if (-not $HelperSource.EndsWith("`n")) {
      $recursiveHelperLineTotal++
    }
    if ($moduleTargetLineTotal -ne 510 -or $recursiveHelperLineTotal -ne 510) {
      throw 'Module guard did not retain the 510-line non-.rs Cargo target and recursive helper module'
    }
  }

  $referenceText = @'
path,lines,limit,kind
crates/sample/src/alpha.rs,900,500,production
crates/sample/src/beta.rs,700,500,production
'@
  $allowedText = @'
path,lines,limit,kind
crates/sample/src/alpha.rs,850,500,production
'@
  $addedText = @'
path,lines,limit,kind
crates/sample/src/alpha.rs,850,500,production
crates/sample/src/beta.rs,700,500,production
crates/sample/src/gamma.rs,501,500,production
'@
  $increasedText = @'
path,lines,limit,kind
crates/sample/src/alpha.rs,901,500,production
crates/sample/src/beta.rs,700,500,production
'@

  $reference = ConvertFrom-ModuleBaselineText -Content $referenceText -Source 'self-test reference'
  $allowed = ConvertFrom-ModuleBaselineText -Content $allowedText -Source 'self-test allowed'
  if (@(Get-BaselineTransitionFailures -CurrentBaseline $allowed -ReferenceBaseline $reference).Count -ne 0) {
    throw 'Module-size baseline transition rejected an allowed decrease/deletion'
  }

  $empty = ConvertFrom-ModuleBaselineText -Content 'path,lines,limit,kind' -Source 'self-test empty'
  if (@($empty.Rows).Count -ne 0 -or
      @(Get-BaselineTransitionFailures -CurrentBaseline $empty -ReferenceBaseline $reference).Count -ne 0) {
    throw 'Module-size baseline did not accept a header-only zero-debt transition'
  }
  $emptyOutput = @((Write-ModuleBaselineCsv -Rows @()))
  if (($emptyOutput -join "`n") -cne 'path,lines,limit,kind') {
    throw 'Module-size baseline generator did not emit a header-only zero-debt baseline'
  }
  $emptyFiles = New-RustGuardOrdinalDictionary
  $emptyCurrent = New-RustGuardOrdinalDictionary
  $emptyExceptions = ConvertFrom-ModuleExceptionsText `
    -Content 'path,owner,reason,expires' `
    -Source 'self-test empty exceptions' `
    -ProductionFilesByPath $emptyFiles `
    -CurrentByPath $emptyCurrent `
    -BaselineByPath $empty.ByPath
  if (@(
      Get-CurrentBaselineFailures `
        -BaselineDocument $empty `
        -ProductionFilesByPath $emptyFiles `
        -CurrentByPath $emptyCurrent
    ).Count -ne 0 -or
      @(
        Get-ModulePolicyFailures `
          -CurrentRows @() `
          -BaselineByPath $empty.ByPath `
          -ExceptionsByPath $emptyExceptions.ByPath
      ).Count -ne 0) {
    throw 'Module-size full zero-debt policy path rejected header-only baseline/exceptions with current=0'
  }

  foreach ($invalid in @(
    (ConvertFrom-ModuleBaselineText -Content $addedText -Source 'self-test added'),
    (ConvertFrom-ModuleBaselineText -Content $increasedText -Source 'self-test increased')
  )) {
    if (@(Get-BaselineTransitionFailures -CurrentBaseline $invalid -ReferenceBaseline $reference).Count -eq 0) {
      throw 'Module-size baseline transition accepted an added or increased path'
    }
  }

  $mutatedByPath = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $mutatedRow = [PSCustomObject]@{
    path = 'crates/sample/src/alpha.rs'
    lines = 850
    limit = 200
    kind = 'module-root'
  }
  $mutatedByPath.Add($mutatedRow.path, $mutatedRow)
  $mutated = [PSCustomObject]@{
    Rows = @($mutatedRow)
    ByPath = $mutatedByPath
  }
  if (@(Get-BaselineTransitionFailures -CurrentBaseline $mutated -ReferenceBaseline $reference).Count -eq 0) {
    throw 'Module-size baseline transition accepted a limit/kind mutation'
  }

  Assert-SelfTestThrows -Name 'non-exact header' -Action {
    ConvertFrom-ModuleBaselineText -Content "path,lines`ncrates/sample/src/a.rs,501" -Source 'self-test header'
  }
  Assert-SelfTestThrows -Name 'extra row column' -Action {
    ConvertFrom-ModuleBaselineText -Content @'
path,lines,limit,kind
crates/sample/src/a.rs,501,500,production,hidden
'@ -Source 'self-test extra column'
  }
  Assert-SelfTestThrows -Name 'non-ordinal baseline' -Action {
    ConvertFrom-ModuleBaselineText -Content @'
path,lines,limit,kind
crates/sample/src/z.rs,501,500,production
crates/sample/src/a.rs,501,500,production
'@ -Source 'self-test order'
  }
  Assert-SelfTestThrows -Name 'non-canonical positive integer' -Action {
    ConvertFrom-ModuleBaselineText -Content @'
path,lines,limit,kind
crates/sample/src/a.rs,+501,500,production
'@ -Source 'self-test integer'
  }

  $caseChanged = ConvertFrom-ModuleBaselineText -Content @'
path,lines,limit,kind
crates/sample/src/Alpha.rs,850,500,production
'@ -Source 'self-test case change'
  if (@(Get-BaselineTransitionFailures -CurrentBaseline $caseChanged -ReferenceBaseline $reference).Count -eq 0) {
    throw 'Module-size baseline transition accepted a case-only identity change'
  }

  $productionMetadata = Get-ModuleMetadataForRepositoryPath -Path 'crates/sample/src/normal.rs'
  $moduleRootMetadata = Get-ModuleMetadataForRepositoryPath -Path 'crates/sample/src/mod.rs'
  if (Test-IsModuleViolation -Lines 500 -Metadata $productionMetadata) {
    throw 'Normal production module at 500 lines was treated as a violation'
  }
  if (-not (Test-IsModuleViolation -Lines 501 -Metadata $productionMetadata)) {
    throw 'Normal production module at 501 lines was not treated as a violation'
  }
  if (Test-IsModuleViolation -Lines 200 -Metadata $moduleRootMetadata) {
    throw 'mod.rs/lib.rs at 200 lines was treated as a violation'
  }
  if (-not (Test-IsModuleViolation -Lines 201 -Metadata $moduleRootMetadata)) {
    throw 'mod.rs/lib.rs at 201 lines was not treated as a violation'
  }

  $emptyBaseline = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $approvedException = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $approvedException.Add('crates/sample/src/normal.rs', $true)

  foreach ($lineCount in @(501, 800)) {
    $row = [PSCustomObject]@{
      path = 'crates/sample/src/normal.rs'
      lines = $lineCount
      limit = 500
      kind = 'production'
    }
    if (@(Get-ModulePolicyFailures -CurrentRows @($row) -BaselineByPath $emptyBaseline -ExceptionsByPath $approvedException).Count -ne 0) {
      throw "A formal exception did not authorize the valid normal-module boundary $lineCount"
    }
  }

  $withoutException = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $row501 = [PSCustomObject]@{
    path = 'crates/sample/src/normal.rs'
    lines = 501
    limit = 500
    kind = 'production'
  }
  if (@(Get-ModulePolicyFailures -CurrentRows @($row501) -BaselineByPath $emptyBaseline -ExceptionsByPath $withoutException).Count -eq 0) {
    throw 'A new 501-line normal module passed without a formal exception'
  }

  $row801 = [PSCustomObject]@{
    path = 'crates/sample/src/normal.rs'
    lines = 801
    limit = 500
    kind = 'production'
  }
  if (@(Get-ModulePolicyFailures -CurrentRows @($row801) -BaselineByPath $emptyBaseline -ExceptionsByPath $approvedException).Count -eq 0) {
    throw 'A formal exception authorized a new module above the 800-line hard ceiling'
  }

  $moduleException = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $moduleException.Add('crates/sample/src/mod.rs', $true)
  $row201 = [PSCustomObject]@{
    path = 'crates/sample/src/mod.rs'
    lines = 201
    limit = 200
    kind = 'module-root'
  }
  if (@(Get-ModulePolicyFailures -CurrentRows @($row201) -BaselineByPath $emptyBaseline -ExceptionsByPath $moduleException).Count -eq 0) {
    throw 'A formal exception authorized a new mod.rs/lib.rs above 200 lines'
  }

  $staleText = @'
path,lines,limit,kind
crates/sample/src/stale.rs,501,500,production
'@
  $stale = ConvertFrom-ModuleBaselineText -Content $staleText -Source 'self-test stale'
  $emptyFiles = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  $emptyCurrent = [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
  if (@(Get-CurrentBaselineFailures -BaselineDocument $stale -ProductionFilesByPath $emptyFiles -CurrentByPath $emptyCurrent).Count -eq 0) {
    throw 'A stale module baseline row was accepted'
  }

  if (Test-IsExcludedRustRepositoryPath -Path 'crates/sample/src/generated/huge.rs') {
    throw 'A handwritten src/generated module was incorrectly excluded by directory name'
  }
  if (Test-IsExcludedRustRepositoryPath -Path 'crates/sample/src/tests/unit.rs') {
    throw 'A src/tests module was incorrectly excluded from module-size scanning'
  }
  if (Test-IsExcludedRustRepositoryPath -Path 'crates/sample/src/test_helpers.rs') {
    throw 'A test-named file under src was incorrectly excluded from module-size scanning'
  }

  $bootstrapRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('meow-module-bootstrap-' + [guid]::NewGuid().ToString('N'))
  try {
    [void][System.IO.Directory]::CreateDirectory($bootstrapRoot)
    $bootstrapBaseline = Join-Path $bootstrapRoot 'baseline.csv'
    $bootstrapManifest = Join-Path $bootstrapRoot 'bootstrap.csv'
    [System.IO.File]::WriteAllText($bootstrapBaseline, $allowedText + "`n", $StrictUtf8)
    $bootstrapHash = Get-Sha256Hex -Bytes ([System.IO.File]::ReadAllBytes($bootstrapBaseline))
    [System.IO.File]::WriteAllText(
      $bootstrapManifest,
      "referenceRevision,baselineSha256`n$InitialBootstrapReference,$bootstrapHash`n",
      $StrictUtf8
    )
    Assert-BootstrapManifest `
      -ReferenceCommit $InitialBootstrapReference `
      -ExpectedSha256 $bootstrapHash `
      -ManifestPath $bootstrapManifest `
      -BaselineFilePath $bootstrapBaseline
    Assert-SelfTestThrows -Name 'missing trusted bootstrap hash' -Action {
      Assert-BootstrapManifest `
        -ReferenceCommit $InitialBootstrapReference `
        -ExpectedSha256 '' `
        -ManifestPath $bootstrapManifest `
        -BaselineFilePath $bootstrapBaseline
    }
    Assert-SelfTestThrows -Name 'mismatched trusted bootstrap hash' -Action {
      Assert-BootstrapManifest `
        -ReferenceCommit $InitialBootstrapReference `
        -ExpectedSha256 ('0' * 64) `
        -ManifestPath $bootstrapManifest `
        -BaselineFilePath $bootstrapBaseline
    }
  } finally {
    if (Test-Path -LiteralPath $bootstrapRoot) {
      Remove-Item -LiteralPath $bootstrapRoot -Recurse -Force
    }
  }

  Write-Host 'Rust module size self-test passed: trusted bootstrap, case-safe physical identity, taskkill/job/snapshot tree cleanup, normal-process isolation, non-.rs target recursion, path/include rejection, zero-debt policy, CSV, and transitions covered'
}

if ($SelfTest) {
  Invoke-ModuleGuardSelfTest
  return
}

$productionFiles = @(Get-ProductionRustFiles)
$productionFilesByPath = [System.Collections.Generic.Dictionary[string,object]]::new(
  [System.StringComparer]::Ordinal
)
foreach ($file in $productionFiles) {
  $path = Get-NormalizedRelativePath -FullName $file.FullName
  if ($productionFilesByPath.ContainsKey($path)) {
    throw "Production Rust source scanner produced a duplicate path: $path"
  }
  $productionFilesByPath.Add($path, $file)
}

$current = @(Get-CurrentModuleViolations -Files $productionFiles)
if ($GenerateBaseline) {
  Write-ModuleBaselineCsv -Rows $current
  return
}

if (-not (Test-Path -LiteralPath $BaselinePath -PathType Leaf)) {
  throw "Module-size baseline is missing: $BaselinePath"
}
$baselineDocument = ConvertFrom-ModuleBaselineText `
  -Content (Read-StrictUtf8Text -Path $BaselinePath) `
  -Source $BaselinePath
$baseline = @($baselineDocument.Rows)
$baselineByPath = $baselineDocument.ByPath

$currentByPath = [System.Collections.Generic.Dictionary[string,object]]::new(
  [System.StringComparer]::Ordinal
)
foreach ($row in $current) {
  if ($currentByPath.ContainsKey([string]$row.path)) {
    throw "Module-size scanner produced a duplicate violation path: $($row.path)"
  }
  $currentByPath.Add([string]$row.path, $row)
}

$currentBaselineFailures = @(
  Get-CurrentBaselineFailures `
    -BaselineDocument $baselineDocument `
    -ProductionFilesByPath $productionFilesByPath `
    -CurrentByPath $currentByPath
)
if ($currentBaselineFailures.Count -gt 0) {
  throw ("Module-size baseline is stale or inconsistent:{0}{1}" -f
    [Environment]::NewLine,
    ($currentBaselineFailures -join [Environment]::NewLine))
}

$referenceCommit = Resolve-ReferenceCommit
Assert-BaselineTransition -CurrentBaseline $baselineDocument -ReferenceCommit $referenceCommit

if (-not (Test-Path -LiteralPath $ExceptionPath -PathType Leaf)) {
  throw "Module-size exception list is missing: $ExceptionPath"
}
$exceptionDocument = ConvertFrom-ModuleExceptionsText `
  -Content (Read-StrictUtf8Text -Path $ExceptionPath) `
  -Source $ExceptionPath `
  -ProductionFilesByPath $productionFilesByPath `
  -CurrentByPath $currentByPath `
  -BaselineByPath $baselineByPath

$failures = @(
  Get-ModulePolicyFailures `
    -CurrentRows $current `
    -BaselineByPath $baselineByPath `
    -ExceptionsByPath $exceptionDocument.ByPath
)
if ($failures.Count -gt 0) {
  Write-Error ("Rust module size guard failed:{0}{1}" -f
    [Environment]::NewLine,
    ($failures -join [Environment]::NewLine))
}

Write-Host ('Rust module size guard passed: current violations={0}, baseline violations={1}, formal exceptions={2}, thresholds target={3}, hard={4}, mod/lib={5}' -f
  $current.Count,
  $baseline.Count,
  $exceptionDocument.Rows.Count,
  $TargetLines,
  $HardLines,
  $ModuleRootHardLines)
