# Requires -Version 5.1
<#
.SYNOPSIS
  CI guard: lock per-crate dependency and production source-size budgets.
.DESCRIPTION
  Discovers workspace members through cargo metadata, counts normal internal
  dependencies and lines under each crate's src tree, and compares them with
  scripts/baselines/crate-budget-policy.csv. Every policy row names an owner,
  a reason, and an expiry date; missing or expired rows fail the guard.

  Use -SelfTest to exercise CSV, budget, and expiry policy without writing
  repository files.
#>
param(
  [string]$PolicyPath,
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'lib/RustGuard.Common.ps1')
if ([string]::IsNullOrWhiteSpace($PolicyPath)) {
  $PolicyPath = Join-Path $repoRoot 'scripts/baselines/crate-budget-policy.csv'
}

$StrictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)
$PolicyHeader = 'crate,maxInternalDeps,maxSrcLoc,owner,reason,expires'
$RequiredCrates = @(
  'analysis-core',
  'app-services',
  'ceph-reconstruction',
  'emulation-services',
  'evidence-access',
  'import-core',
  'source-kernel',
  'source-reconstruction'
)

function Read-StrictUtf8Text {
  param([Parameter(Mandatory = $true)][string]$Path)

  try {
    return $StrictUtf8.GetString([System.IO.File]::ReadAllBytes($Path))
  } catch {
    throw "File is not valid UTF-8: $Path"
  }
}

function Get-LineCountFromText {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Content)

  if ($Content.Length -eq 0) {
    return 0
  }
  $lineFeeds = ([regex]::Matches($Content, "`n")).Count
  if ($Content.EndsWith("`n")) {
    return $lineFeeds
  }

  return $lineFeeds + 1
}

function Get-SourceLineCount {
  param([Parameter(Mandatory = $true)][string]$Root)

  $src = Join-Path $Root 'src'
  if (-not (Test-Path -LiteralPath $src -PathType Container)) {
    return 0
  }
  $total = 0
  foreach ($file in Get-ChildItem -LiteralPath $src -Recurse -File -Filter '*.rs') {
    $total += Get-LineCountFromText -Content (Read-StrictUtf8Text -Path $file.FullName)
  }

  return $total
}

function Get-InternalDependencyNames {
  param(
    [Parameter(Mandatory = $true)]$Package,
    [Parameter(Mandatory = $true)][System.Collections.Generic.HashSet[string]]$MemberNames
  )

  $names = New-Object System.Collections.Generic.List[string]
  foreach ($dependency in @($Package.dependencies)) {
    if ($null -ne $dependency.kind) {
      continue
    }
    $name = [string]$dependency.name
    if ($MemberNames.Contains($name)) {
      $names.Add($name)
    }
  }

  return @($names | Sort-Object -Unique)
}

function ConvertFrom-BudgetPolicyText {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Source
  )

  $firstLine = @($Content -split '\r?\n')[0]
  if ($firstLine -cne $PolicyHeader) {
    throw "Crate-budget policy at $Source must use the exact header: $PolicyHeader"
  }

  $rows = @($Content | ConvertFrom-Csv)
  $byCrate = New-Object 'System.Collections.Generic.Dictionary[string,object]' ([System.StringComparer]::Ordinal)
  $previous = $null
  $today = [DateTime]::UtcNow.Date
  foreach ($row in $rows) {
    foreach ($field in @('crate', 'maxInternalDeps', 'maxSrcLoc', 'owner', 'reason', 'expires')) {
      if ([string]::IsNullOrWhiteSpace([string]$row.$field)) {
        throw "Crate-budget policy at $Source contains an empty required field '$field'"
      }
    }
    $crate = [string]$row.crate
    if ($crate -notmatch '^[a-z0-9][a-z0-9-]*$') {
      throw "Crate-budget policy at $Source contains an invalid crate name: $crate"
    }
    if ($byCrate.ContainsKey($crate)) {
      throw "Crate-budget policy at $Source contains a duplicate crate: $crate"
    }
    if ($null -ne $previous -and [string]::CompareOrdinal([string]$previous, $crate) -ge 0) {
      throw "Crate-budget policy at $Source is not in deterministic ordinal order near: $crate"
    }

    $maxInternalDeps = 0
    if (-not [int]::TryParse([string]$row.maxInternalDeps, [ref]$maxInternalDeps) -or
        $maxInternalDeps -le 0) {
      throw "Crate-budget policy at $Source has an invalid maxInternalDeps for $crate"
    }
    $maxSrcLoc = 0
    if (-not [int]::TryParse([string]$row.maxSrcLoc, [ref]$maxSrcLoc) -or $maxSrcLoc -le 0) {
      throw "Crate-budget policy at $Source has an invalid maxSrcLoc for $crate"
    }
    $expires = [DateTime]::MinValue
    if (-not [DateTime]::TryParseExact(
        [string]$row.expires,
        'yyyy-MM-dd',
        [System.Globalization.CultureInfo]::InvariantCulture,
        [System.Globalization.DateTimeStyles]::None,
        [ref]$expires)) {
      throw "Crate-budget policy at $Source has an invalid expiry for $crate"
    }
    if ($expires.Date -lt $today) {
      throw "Crate-budget policy at $Source has expired for $crate`: $($row.expires)"
    }

    $byCrate.Add($crate, [PSCustomObject]@{
        crate = $crate
        maxInternalDeps = $maxInternalDeps
        maxSrcLoc = $maxSrcLoc
        owner = [string]$row.owner
        reason = [string]$row.reason
        expires = [string]$row.expires
      })
    $previous = $crate
  }

  return [PSCustomObject]@{
    Rows = $rows
    ByCrate = $byCrate
  }
}

function Get-BudgetFailures {
  param(
    [Parameter(Mandatory = $true)]$Policy,
    [Parameter(Mandatory = $true)][System.Collections.Generic.Dictionary[string,object]]$PackagesByName,
    [Parameter(Mandatory = $true)][System.Collections.Generic.HashSet[string]]$MemberNames,
    [AllowEmptyCollection()][string[]]$Required = $RequiredCrates
  )

  $failures = New-Object System.Collections.Generic.List[string]
  foreach ($crate in $Required) {
    if (-not $Policy.ByCrate.ContainsKey($crate)) {
      $failures.Add("missing crate-budget policy row: $crate")
    }
  }

  foreach ($crate in $Policy.ByCrate.Keys) {
    if (-not $PackagesByName.ContainsKey($crate)) {
      $failures.Add("crate-budget policy references a non-member crate: $crate")
      continue
    }
    $package = $PackagesByName[$crate]
    $row = $Policy.ByCrate[$crate]
    $dependencyCount = @(Get-InternalDependencyNames -Package $package -MemberNames $MemberNames).Count
    if ($dependencyCount -gt [int]$row.maxInternalDeps) {
      $failures.Add("$crate has $dependencyCount internal dependencies; budget is $($row.maxInternalDeps)")
    }
    $loc = Get-SourceLineCount -Root (Split-Path -Parent ([string]$package.manifest_path))
    if ($loc -gt [int]$row.maxSrcLoc) {
      $failures.Add("$crate src has $loc lines; budget is $($row.maxSrcLoc)")
    }
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
    throw "Crate-budget self-test expected failure: $Name"
  }
}

function Invoke-BudgetSelfTest {
  if ((Get-LineCountFromText "alpha`n") -ne 1 -or
      (Get-LineCountFromText "alpha`nbeta") -ne 2 -or
      (Get-LineCountFromText '') -ne 0) {
    throw 'Crate-budget self-test failed: line counting is incorrect'
  }

  $validText = @'
crate,maxInternalDeps,maxSrcLoc,owner,reason,expires
analysis-core,10,2000,architecture,self test,2999-12-31
app-services,20,20000,architecture,self test,2999-12-31
'@
  $policy = ConvertFrom-BudgetPolicyText -Content $validText -Source 'self-test valid'
  if ($policy.Rows.Count -ne 2 -or
      [int]$policy.ByCrate['analysis-core'].maxInternalDeps -ne 10) {
    throw 'Crate-budget self-test failed: valid policy was not parsed'
  }

  foreach ($invalid in @(
      "crate,maxInternalDeps,maxSrcLoc`nanalysis-core,10,2000",
      "crate,maxInternalDeps,maxSrcLoc,owner,reason,expires`nanalysis-core,0,2000,a,b,2999-12-31",
      "crate,maxInternalDeps,maxSrcLoc,owner,reason,expires`nanalysis-core,10,-1,a,b,2999-12-31",
      "crate,maxInternalDeps,maxSrcLoc,owner,reason,expires`nanalysis-core,10,2000,,b,2999-12-31",
      "crate,maxInternalDeps,maxSrcLoc,owner,reason,expires`nanalysis-core,10,2000,a,b,2000-01-01",
      "crate,maxInternalDeps,maxSrcLoc,owner,reason,expires`nanalysis-core,10,2000,a,b,2999-12-31`nanalysis-core,11,2000,a,b,2999-12-31",
      "crate,maxInternalDeps,maxSrcLoc,owner,reason,expires`nz-crate,10,2000,a,b,2999-12-31`nanalysis-core,10,2000,a,b,2999-12-31"
    )) {
    Assert-SelfTestThrows -Name 'invalid policy row' -Action {
      [void](ConvertFrom-BudgetPolicyText -Content $invalid -Source 'self-test invalid')
    }
  }

  $memberNames = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
  [void]$memberNames.Add('analysis-core')
  [void]$memberNames.Add('app-services')
  $packages = New-Object 'System.Collections.Generic.Dictionary[string,object]' ([System.StringComparer]::Ordinal)
  $packages.Add('analysis-core', [PSCustomObject]@{
      name = 'analysis-core'
      manifest_path = (Join-Path $PSScriptRoot '../crates/analysis-core/Cargo.toml')
      dependencies = @()
    })
  $packages.Add('app-services', [PSCustomObject]@{
      name = 'app-services'
      manifest_path = (Join-Path $PSScriptRoot '../crates/app-services/Cargo.toml')
      dependencies = @()
    })
  $underPolicy = ConvertFrom-BudgetPolicyText -Content @'
crate,maxInternalDeps,maxSrcLoc,owner,reason,expires
analysis-core,10,999999,architecture,self test,2999-12-31
app-services,20,999999,architecture,self test,2999-12-31
'@ -Source 'self-test under budget'
  if (@(Get-BudgetFailures -Policy $underPolicy -PackagesByName $packages -MemberNames $memberNames -Required @('analysis-core', 'app-services')).Count -ne 0) {
    throw 'Crate-budget self-test rejected a valid policy'
  }

  $missingRow = ConvertFrom-BudgetPolicyText -Content @'
crate,maxInternalDeps,maxSrcLoc,owner,reason,expires
analysis-core,10,999999,architecture,self test,2999-12-31
'@ -Source 'self-test missing row'
  if (@(Get-BudgetFailures -Policy $missingRow -PackagesByName $packages -MemberNames $memberNames -Required @('analysis-core', 'app-services')).Count -eq 0) {
    throw 'Crate-budget self-test accepted a missing required policy row'
  }

  Write-Host 'Crate-budget self-test passed'
}

if ($SelfTest) {
  Invoke-BudgetSelfTest
  return
}

if (-not (Test-Path -LiteralPath $PolicyPath -PathType Leaf)) {
  throw "Crate-budget policy is missing: $PolicyPath"
}
$policy = ConvertFrom-BudgetPolicyText -Content (Read-StrictUtf8Text -Path $PolicyPath) -Source $PolicyPath

$metadata = Invoke-RustGuardCargoMetadata -RepoRoot $repoRoot
$packagesByName = New-Object 'System.Collections.Generic.Dictionary[string,object]' ([System.StringComparer]::Ordinal)
$memberNames = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::Ordinal)
foreach ($package in @($metadata.packages)) {
  $name = [string]$package.name
  $packagesByName[$name] = $package
  [void]$memberNames.Add($name)
}

$failures = @(Get-BudgetFailures -Policy $policy -PackagesByName $packagesByName -MemberNames $memberNames)
if ($failures.Count -gt 0) {
  Write-Error ("Crate-budget guard failed:`n" + ($failures -join "`n"))
}

Write-Host ('Crate-budget guard passed: {0} governed crates' -f $policy.Rows.Count)
