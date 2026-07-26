#Requires -Version 5.1
<#
.SYNOPSIS
  Prevent private real-sample Rust tests from polluting default test gates.
.DESCRIPTION
  Scans physical Rust test trees. A test that directly or transitively reads a
  private fixture environment variable must carry #[ignore]. Private fixture
  providers may not fall back to concrete Windows drive paths, and local-run
  examples may not expose those paths. Compile-time include_str!/include_bytes!
  dependencies on ignored private testdata directories are also prohibited.
  Public repository fixtures, synthetic PathBuf values, and Z:/ non-existent-
  path tests are intentionally unaffected.
#>
param(
  [string]$ScanRoot,
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'lib/RustGuard.Common.ps1')

function Test-PrivateFixtureEnvironmentName {
  param([Parameter(Mandatory = $true)][string]$Name)

  return $Name -cmatch '^FORENSICS_(?:[A-Z0-9_]*E01(?:_FIXTURE)?|PVE_[A-Z0-9_]*(?:FIXTURE|ROOT)|EMAIL_FIXTURE_DIR|REAL_IMAGE_DIR|[A-Z0-9_]+_ORACLE)$'
}

function Get-LineNumber {
  param(
    [Parameter(Mandatory = $true)][string]$Text,
    [Parameter(Mandatory = $true)][int]$Index
  )

  if ($Index -le 0) {
    return 1
  }
  return 1 + [regex]::Matches($Text.Substring(0, $Index), "`n").Count
}

function Read-StrictUtf8TestSource {
  param([Parameter(Mandatory = $true)][string]$Path)

  $encoding = New-Object System.Text.UTF8Encoding($false, $true)
  try {
    return $encoding.GetString([System.IO.File]::ReadAllBytes($Path))
  } catch {
    throw "Rust test source is not valid UTF-8: $Path"
  }
}

function Get-RustTestFunctions {
  param([Parameter(Mandatory = $true)][string]$Source)

  $masked = [Stage0.RustGuardLexicalMasker]::Mask($Source)
  $pattern = '(?ms)(?<attrs>(?:^[ \t]*#\[[^\r\n]*\][ \t]*\r?\n)*)^[ \t]*(?:(?:pub(?:[ \t]*\([^\r\n)]*\))?|async|unsafe|const|extern)[ \t]+)*fn[ \t]+(?<name>(?:r#)?[A-Za-z_][A-Za-z0-9_]*)[^;{]*\{'
  $functions = New-Object System.Collections.Generic.List[object]

  foreach ($match in [regex]::Matches($masked, $pattern)) {
    $openBrace = $match.Index + $match.Length - 1
    $depth = 0
    $closeBrace = -1
    for ($index = $openBrace; $index -lt $masked.Length; $index++) {
      if ($masked[$index] -eq '{') {
        $depth++
      } elseif ($masked[$index] -eq '}') {
        $depth--
        if ($depth -eq 0) {
          $closeBrace = $index
          break
        }
      }
    }
    if ($closeBrace -lt 0) {
      continue
    }

    $bodyStart = $openBrace + 1
    $bodyLength = $closeBrace - $bodyStart
    $maskedBody = $masked.Substring($bodyStart, $bodyLength)
    $calledFunctions = [System.Collections.Generic.HashSet[string]]::new(
      [System.StringComparer]::Ordinal
    )
    foreach ($callMatch in [regex]::Matches(
        $maskedBody,
        '(?<![A-Za-z0-9_])(?<name>(?:r#)?[A-Za-z_][A-Za-z0-9_]*)\s*\(')) {
      [void]$calledFunctions.Add($callMatch.Groups['name'].Value)
    }
    $attributes = $Source.Substring(
      $match.Groups['attrs'].Index,
      $match.Groups['attrs'].Length
    )
    $functions.Add([pscustomobject]@{
      Name = $match.Groups['name'].Value
      Line = Get-LineNumber -Text $Source -Index $match.Index
      Attributes = $attributes
      Body = $Source.Substring($bodyStart, $bodyLength)
      MaskedBody = $maskedBody
      CalledFunctions = $calledFunctions
      IsTest = $attributes -cmatch '#\s*\[\s*(?:(?:tokio|async_std)::)?test(?:\s*\(|\s*\])'
      IsIgnored = $attributes -cmatch '#\s*\[\s*ignore(?:\s*=|\s*\])'
      DirectPrivateDependency = $false
    })
  }

  return $functions.ToArray()
}

function Get-PrivateEnvironmentConstants {
  param([Parameter(Mandatory = $true)][string]$Source)

  $names = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::Ordinal
  )
  $pattern = '(?m)^\s*const\s+(?<name>[A-Z][A-Z0-9_]*)\s*:[^=\r\n]+?=\s*"(?<value>FORENSICS_[A-Z0-9_]+)"\s*;'
  foreach ($match in [regex]::Matches($Source, $pattern)) {
    if (Test-PrivateFixtureEnvironmentName -Name $match.Groups['value'].Value) {
      [void]$names.Add($match.Groups['name'].Value)
    }
  }
  return ,$names
}

function Get-AbsoluteDrivePathConstants {
  param([Parameter(Mandatory = $true)][string]$Source)

  $names = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::Ordinal
  )
  $pattern = '(?m)^\s*const\s+(?<name>[A-Z][A-Z0-9_]*)\s*:[^=\r\n]+?=\s*(?:r#*)?"[A-Za-z]:(?:\\\\|\\|/)'
  foreach ($match in [regex]::Matches($Source, $pattern)) {
    [void]$names.Add($match.Groups['name'].Value)
  }
  return ,$names
}

function Test-BodyHasPrivateDependency {
  param(
    [Parameter(Mandatory = $true)][object]$Function,
    [Parameter(Mandatory = $true)]$PrivateEnvironmentConstants
  )

  foreach ($match in [regex]::Matches($Function.Body, 'FORENSICS_[A-Z0-9_]+')) {
    if (Test-PrivateFixtureEnvironmentName -Name $match.Value) {
      return $true
    }
  }
  foreach ($constant in $PrivateEnvironmentConstants) {
    if ($Function.MaskedBody -cmatch "(?<![A-Za-z0-9_])$([regex]::Escape($constant))(?![A-Za-z0-9_])") {
      return $true
    }
  }
  return $false
}

function Test-BodyCallsFunction {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$MaskedBody,
    [Parameter(Mandatory = $true)][string]$FunctionName
  )

  $escaped = [regex]::Escape($FunctionName)
  return $MaskedBody -cmatch "(?<![A-Za-z0-9_])$escaped\s*\("
}

function Get-RustModuleAliases {
  param([Parameter(Mandatory = $true)][string]$RelativePath)

  $aliases = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::Ordinal
  )
  $segments = $RelativePath.Replace('\', '/') -split '/'
  if ($segments.Count -eq 0) {
    return ,$aliases
  }

  $fileName = $segments[-1]
  $stem = [System.IO.Path]::GetFileNameWithoutExtension($fileName)
  if ($stem -ceq 'mod' -and $segments.Count -gt 1) {
    $stem = $segments[-2]
  }
  if ($stem -cmatch '^[A-Za-z_][A-Za-z0-9_]*$') {
    [void]$aliases.Add($stem)
  }
  return ,$aliases
}

function Test-BodyCallsCrossFileFunction {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$MaskedBody,
    [Parameter(Mandatory = $true)][string]$MaskedSource,
    [Parameter(Mandatory = $true)][string]$FunctionName,
    [Parameter(Mandatory = $true)]$ModuleAliases
  )

  $escapedFunction = [regex]::Escape($FunctionName)
  foreach ($moduleAlias in $ModuleAliases) {
    $escapedAlias = [regex]::Escape($moduleAlias)
    $pathSegment = '[A-Za-z_][A-Za-z0-9_]*\s*::\s*'
    $qualifiedCall = "(?<![A-Za-z0-9_])(?:$pathSegment)*$escapedAlias\s*::\s*(?:$pathSegment)*$escapedFunction\s*\("
    if ($MaskedBody -cmatch $qualifiedCall) {
      return $true
    }

    $directImport = "(?m)^\s*use\s+(?:$pathSegment)*$escapedAlias\s*::\s*(?:$escapedFunction|\{[^}\r\n]*(?<![A-Za-z0-9_])$escapedFunction(?![A-Za-z0-9_])[^}\r\n]*\})\s*;"
    $globImport = "(?m)^\s*use\s+(?:$pathSegment)*$escapedAlias\s*::\s*\*\s*;"
    if (($MaskedSource -cmatch $directImport -or $MaskedSource -cmatch $globImport) -and
        (Test-BodyCallsFunction -MaskedBody $MaskedBody -FunctionName $FunctionName)) {
      return $true
    }
  }
  return $false
}

function Get-TestFiles {
  param([Parameter(Mandatory = $true)][string]$Root)

  $rootPath = [System.IO.Path]::GetFullPath($Root)
  return @(
    Get-ChildItem -LiteralPath $rootPath -Recurse -File -Filter '*.rs' |
      Where-Object {
        $relative = Get-RustGuardRepositoryRelativePath -RepoRoot $rootPath -FullName $_.FullName
        $relative -cmatch '(^|/)tests/' -and
          $relative -cnotmatch '(^|/)(?:target|\.git|\.claude|\.codex)/'
      } |
      Sort-Object -Property FullName
  )
}

function Get-IgnoredPrivateTestdataDirectories {
  param([Parameter(Mandatory = $true)][string]$Root)

  $directories = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
  )
  $gitignore = Join-Path $Root '.gitignore'
  if (-not (Test-Path -LiteralPath $gitignore -PathType Leaf)) {
    return ,$directories
  }

  foreach ($line in [System.IO.File]::ReadAllLines($gitignore)) {
    $trimmed = $line.Trim()
    if ($trimmed -cmatch '^/(?<path>testdata/[^*?!#]+/)$') {
      [void]$directories.Add($Matches['path'].Replace('\', '/'))
    }
  }
  return ,$directories
}

function Find-CompileTimePrivateFixtureIncludes {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)]$IgnoredDirectories,
    [Parameter(Mandatory = $true)][string]$RelativePath
  )

  $violations = New-Object System.Collections.Generic.List[string]
  if ($IgnoredDirectories.Count -eq 0) {
    return $violations.ToArray()
  }

  $masked = [Stage0.RustGuardLexicalMasker]::Mask($Source)
  $pattern = '(?<![A-Za-z0-9_])include_(?:str|bytes)\s*!\s*\('
  foreach ($match in [regex]::Matches($masked, $pattern)) {
    $openParen = $match.Index + $match.Length - 1
    $depth = 0
    $closeParen = -1
    for ($index = $openParen; $index -lt $masked.Length; $index++) {
      if ($masked[$index] -eq '(') {
        $depth++
      } elseif ($masked[$index] -eq ')') {
        $depth--
        if ($depth -eq 0) {
          $closeParen = $index
          break
        }
      }
    }
    if ($closeParen -lt 0) {
      continue
    }

    $invocation = $Source.Substring($match.Index, $closeParen - $match.Index + 1)
    $normalized = $invocation.Replace('\', '/')
    foreach ($directory in $IgnoredDirectories) {
      if ($normalized.IndexOf($directory, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
        $line = Get-LineNumber -Text $Source -Index $match.Index
        $violations.Add(
          "[compile-time-private-fixture] ${RelativePath}:$line include_str!/include_bytes! cannot depend on ignored /$directory"
        )
        break
      }
    }
  }

  return $violations.ToArray()
}

function Find-PrivateSampleTestViolations {
  param([Parameter(Mandatory = $true)][string]$Root)

  $violations = New-Object System.Collections.Generic.List[string]
  $drivePathPattern = '(?<![A-Za-z0-9_])[A-Za-z]:(?:\\\\|\\|/)'
  $ignoredPrivateDirectories = Get-IgnoredPrivateTestdataDirectories -Root $Root
  $records = New-Object System.Collections.Generic.List[object]

  foreach ($file in Get-TestFiles -Root $Root) {
    $relative = Get-RustGuardRepositoryRelativePath -RepoRoot $Root -FullName $file.FullName
    $source = Read-StrictUtf8TestSource -Path $file.FullName
    foreach ($violation in Find-CompileTimePrivateFixtureIncludes `
        -Source $source `
        -IgnoredDirectories $ignoredPrivateDirectories `
        -RelativePath $relative) {
      $violations.Add($violation)
    }
    $functions = @(Get-RustTestFunctions -Source $source)
    $privateConstants = Get-PrivateEnvironmentConstants -Source $source
    $absolutePathConstants = Get-AbsoluteDrivePathConstants -Source $source
    $records.Add([pscustomobject]@{
      RelativePath = $relative
      Source = $source
      MaskedSource = [Stage0.RustGuardLexicalMasker]::Mask($source)
      Functions = $functions
      PrivateConstants = $privateConstants
      AbsolutePathConstants = $absolutePathConstants
      ModuleAliases = Get-RustModuleAliases -RelativePath $relative
    })

    $lines = $source -split "`n"
    for ($lineIndex = 0; $lineIndex -lt $lines.Count; $lineIndex++) {
      $line = $lines[$lineIndex].TrimEnd("`r")
      if ($line -cnotmatch $drivePathPattern) {
        continue
      }

      $hasPrivateEnvironment = $false
      foreach ($match in [regex]::Matches($line, 'FORENSICS_[A-Z0-9_]+')) {
        if (Test-PrivateFixtureEnvironmentName -Name $match.Value) {
          $hasPrivateEnvironment = $true
          break
        }
      }
      $hasKnownPrivateSegment = $line -match '(pangushi|liuyang|evidence-cup|case-material|private-case)'
      if ($hasPrivateEnvironment -or $hasKnownPrivateSegment) {
        $violations.Add(
          "[path-leak] ${relative}:$($lineIndex + 1) replace the concrete private path with an angle-bracket placeholder"
        )
      }
    }
  }

  $privateFunctionKeys = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::Ordinal
  )
  $privateFunctionQueue = New-Object System.Collections.Generic.List[object]
  $callersByFunctionName = @{}
  foreach ($record in $records) {
    foreach ($function in $record.Functions) {
      foreach ($calledFunction in $function.CalledFunctions) {
        if (-not $callersByFunctionName.ContainsKey($calledFunction)) {
          $callersByFunctionName[$calledFunction] = New-Object System.Collections.Generic.List[object]
        }
        $callersByFunctionName[$calledFunction].Add(
          [pscustomobject]@{ Record = $record; Function = $function }
        )
      }
      if (-not (Test-BodyHasPrivateDependency `
          -Function $function `
          -PrivateEnvironmentConstants $record.PrivateConstants)) {
        continue
      }
      $function.DirectPrivateDependency = $true
      $key = "$($record.RelativePath)::$($function.Name)"
      if ($privateFunctionKeys.Add($key)) {
        $privateFunctionQueue.Add([pscustomobject]@{ Record = $record; Function = $function })
      }
    }
  }

  for ($queueIndex = 0; $queueIndex -lt $privateFunctionQueue.Count; $queueIndex++) {
    $privateCallee = $privateFunctionQueue[$queueIndex]
    $privateName = $privateCallee.Function.Name
    if (-not $callersByFunctionName.ContainsKey($privateName)) {
      continue
    }
    foreach ($candidate in $callersByFunctionName[$privateName]) {
      $callerRecord = $candidate.Record
      $caller = $candidate.Function
      $callerKey = "$($callerRecord.RelativePath)::$($caller.Name)"
      if ($privateFunctionKeys.Contains($callerKey)) {
        continue
      }

      if ($callerRecord.RelativePath -ceq $privateCallee.Record.RelativePath) {
        $callsPrivate = $true
      } else {
        $callsPrivate = Test-BodyCallsCrossFileFunction `
          -MaskedBody $caller.MaskedBody `
          -MaskedSource $callerRecord.MaskedSource `
          -FunctionName $privateName `
          -ModuleAliases $privateCallee.Record.ModuleAliases
      }

      if ($callsPrivate -and $privateFunctionKeys.Add($callerKey)) {
        $privateFunctionQueue.Add(
          [pscustomobject]@{ Record = $callerRecord; Function = $caller }
        )
      }
    }
  }

  foreach ($record in $records) {
    foreach ($function in $record.Functions) {
      $functionKey = "$($record.RelativePath)::$($function.Name)"
      if ($function.IsTest -and
          $privateFunctionKeys.Contains($functionKey) -and
          -not $function.IsIgnored) {
        $violations.Add(
          "[missing-ignore] $($record.RelativePath):$($function.Line) private sample test '$($function.Name)' must use #[ignore]"
        )
      }

      if (-not $function.DirectPrivateDependency) {
        continue
      }
      $hasInlinePath = $function.Body -cmatch $drivePathPattern
      $hasPathConstant = $false
      foreach ($constant in $record.AbsolutePathConstants) {
        if ($function.MaskedBody -cmatch "(?<![A-Za-z0-9_])$([regex]::Escape($constant))(?![A-Za-z0-9_])") {
          $hasPathConstant = $true
          break
        }
      }
      if ($hasInlinePath -or $hasPathConstant) {
        $violations.Add(
          "[absolute-fallback] $($record.RelativePath):$($function.Line) private fixture provider '$($function.Name)' contains a Windows drive path fallback"
        )
      }
    }
  }

  return $violations.ToArray()
}

function Write-SelfTestFile {
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$Source
  )

  $tests = Join-Path $Root 'crate/tests'
  [void](New-Item -ItemType Directory -Path $tests -Force)
  $path = Join-Path $tests 'guard_fixture.rs'
  [System.IO.File]::WriteAllText($path, $Source, [System.Text.UTF8Encoding]::new($false))
}

function Write-SelfTestSupportFile {
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$Source
  )

  $path = Join-Path $Root 'crate/tests/support.rs'
  [System.IO.File]::WriteAllText($path, $Source, [System.Text.UTF8Encoding]::new($false))
}

function Invoke-SelfTest {
  $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("private-sample-guard-" + [guid]::NewGuid())
  [void](New-Item -ItemType Directory -Path $temp)
  try {
    [System.IO.File]::WriteAllText(
      (Join-Path $temp '.gitignore'),
      "/testdata/real-samples/`n/testdata/private/`n",
      [System.Text.UTF8Encoding]::new($false)
    )
    Write-SelfTestFile -Root $temp -Source @'
fn private_fixture() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE")
}

#[test]
#[ignore = "private fixture"]
fn private_test_is_opt_in() {
    let _ = private_fixture();
}

#[test]
fn committed_fixture_stays_in_default_gate() {
    let _ = std::path::PathBuf::from("D:/metadata.E01");
    let _ = std::path::Path::new("Z:/path-that-must-not-exist/sample.E01");
}
'@
    $valid = @(Find-PrivateSampleTestViolations -Root $temp)
    if ($valid.Count -ne 0) {
      throw "Private sample guard self-test rejected valid input: $($valid -join '; ')"
    }

    Write-SelfTestFile -Root $temp -Source @'
fn private_fixture() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE")
}

#[test]
fn private_test_runs_by_default() {
    let _ = private_fixture();
}
'@
    $missingIgnore = @(Find-PrivateSampleTestViolations -Root $temp)
    if (-not ($missingIgnore -match '^\[missing-ignore\]')) {
      throw 'Private sample guard self-test did not reject an unignored private test'
    }

    Write-SelfTestFile -Root $temp -Source @'
#[test]
fn private_oracle_runs_by_default() {
    let _ = std::env::var_os("FORENSICS_LINUX_PREVIEW_ORACLE")
        .expect("set FORENSICS_LINUX_PREVIEW_ORACLE");
}
'@
    $oracleIgnore = @(Find-PrivateSampleTestViolations -Root $temp)
    if (-not ($oracleIgnore -match '^\[missing-ignore\]')) {
      throw 'Private sample guard self-test did not classify a private oracle environment variable'
    }

    Write-SelfTestFile -Root $temp -Source @'
fn private_fixture() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("F:/private-case/sample.E01"))
}

#[test]
#[ignore = "private fixture"]
fn private_test_has_local_fallback() {
    let _ = private_fixture();
}
'@
    $fallback = @(Find-PrivateSampleTestViolations -Root $temp)
    if (-not ($fallback -match '^\[absolute-fallback\]')) {
      throw 'Private sample guard self-test did not reject an absolute fixture fallback'
    }

    Write-SelfTestFile -Root $temp -Source @'
// $env:FORENSICS_E01_FIXTURE='C:/private-case/sample.E01'
#[test]
#[ignore = "private fixture"]
fn private_test_uses_explicit_environment() {
    let _ = std::env::var_os("FORENSICS_E01_FIXTURE")
        .expect("set FORENSICS_E01_FIXTURE");
}
'@
    $leak = @(Find-PrivateSampleTestViolations -Root $temp)
    if (-not ($leak -match '^\[path-leak\]')) {
      throw 'Private sample guard self-test did not reject a concrete local-run path'
    }

    Write-SelfTestFile -Root $temp -Source @'
const PRIVATE_ORACLE: &str =
    include_str!("../../../testdata/real-samples/private-oracle.json");

#[test]
#[ignore = "private fixture"]
fn compile_time_private_oracle() {
    assert!(!PRIVATE_ORACLE.is_empty());
}
'@
    $compileTime = @(Find-PrivateSampleTestViolations -Root $temp)
    if (-not ($compileTime -match '^\[compile-time-private-fixture\]')) {
      throw 'Private sample guard self-test did not reject a compile-time private fixture include'
    }

    Write-SelfTestFile -Root $temp -Source @'
mod support;

#[test]
fn cross_module_private_test_runs_by_default() {
    let _ = support::private_fixture();
}
'@
    Write-SelfTestSupportFile -Root $temp -Source @'
pub fn private_fixture() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE")
}
'@
    $crossModule = @(Find-PrivateSampleTestViolations -Root $temp)
    if (-not ($crossModule -match '^\[missing-ignore\].*cross_module_private_test_runs_by_default')) {
      throw 'Private sample guard self-test did not propagate private dependencies across Rust modules'
    }
  } finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
  }

  Write-Host 'Private real-sample test guard self-test passed.'
}

if ($SelfTest) {
  Invoke-SelfTest
  if ([string]::IsNullOrWhiteSpace($ScanRoot)) {
    return
  }
}

if ([string]::IsNullOrWhiteSpace($ScanRoot)) {
  $ScanRoot = $repoRoot
}
$resolvedRoot = (Resolve-Path -LiteralPath $ScanRoot).Path
$violations = @(Find-PrivateSampleTestViolations -Root $resolvedRoot)
if ($violations.Count -gt 0) {
  throw "Private real-sample test guard failed:`n$($violations -join "`n")"
}

Write-Host 'Private real-sample test guard passed.'
