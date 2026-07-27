#Requires -Version 5.1
<#
.SYNOPSIS
  Enforce the BitLocker credential boundary and upstream provenance record.
.DESCRIPTION
  The BitLocker volume layer handles passwords and recovery passwords that must
  never reach persistent storage, logs, events, reports, or the frontend. This
  guard enforces the rules from docs/bitlocker-volume-layer-design.md section 2.4
  structurally, so a leak fails CI rather than a review:

    1. Secret-bearing types must not derive Debug, Clone, or Serialize.
    2. Secret accessors must not be routed into logging or formatting macros.
    3. The crate must not create plaintext temporary files or open evidence
       writable.
    4. unsafe_code must stay forbidden.
    5. docs/bitlocker-dependency-decision.md must keep naming the pinned
       upstream commit, so a silent upstream refresh cannot pass.
    6. Transport DTOs must not contain password, recovery-password,
       credential, or passphrase fields.

  Rule 1 is the one that matters most in practice: a single #[derive(Debug)] on a
  key type plus one tracing call is enough to write a volume key to disk.
#>
param(
  [string]$ScanRoot,
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# The pinned upstream commit. Changing this requires re-recording the per-file
# source checksums in the decision record in the same commit.
$script:PinnedUpstreamCommit = '7c931d4be338a172de9799476eb744ba089e0867'

# Types that hold credential or key material. Adding a secret type means adding
# it here; the guard cannot infer secrecy from a name alone.
$script:SecretTypeNames = @('Passphrase', 'VolumeKeyPackage', 'PersistedKeyBlob')

# Accessors that hand out secret bytes. Any of these appearing inside a logging
# or formatting macro is a leak.
$script:SecretAccessors = @(
  'expose_for_derivation',
  'expose_fvek',
  'expose_tweak',
  'expose_for_storage'
)

function Read-Utf8Text {
  param([Parameter(Mandatory = $true)][string]$Path)

  $encoding = New-Object System.Text.UTF8Encoding($false, $true)
  try {
    return $encoding.GetString([System.IO.File]::ReadAllBytes($Path))
  } catch {
    throw "File is not valid UTF-8: $Path"
  }
}

function Get-LineNumber {
  param(
    [Parameter(Mandatory = $true)][string]$Text,
    [Parameter(Mandatory = $true)][int]$Index
  )

  if ($Index -le 0) { return 1 }
  return 1 + [regex]::Matches($Text.Substring(0, $Index), "`n").Count
}

function Find-SecretDeriveViolations {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Source,
    [Parameter(Mandatory = $true)][string]$RelativePath
  )

  $violations = New-Object System.Collections.Generic.List[string]
  foreach ($typeName in $script:SecretTypeNames) {
    $escaped = [regex]::Escape($typeName)
    # Capture the attribute block immediately preceding the type declaration.
    $pattern = "(?ms)(?<attrs>(?:^[ \t]*#\[[^\r\n]*\][ \t]*\r?\n)*)^[ \t]*(?:pub(?:[ \t]*\([^\r\n)]*\))?[ \t]+)?(?:struct|enum)[ \t]+$escaped(?![A-Za-z0-9_])"
    foreach ($match in [regex]::Matches($Source, $pattern)) {
      $attrs = $match.Groups['attrs'].Value
      foreach ($forbidden in @('Debug', 'Clone', 'Serialize', 'Deserialize')) {
        if ($attrs -cmatch "derive\s*\([^)]*(?<![A-Za-z0-9_])$forbidden(?![A-Za-z0-9_])") {
          $line = Get-LineNumber -Text $Source -Index $match.Index
          $violations.Add(
            "[secret-derive] ${RelativePath}:$line secret type '$typeName' must not derive $forbidden"
          )
        }
      }
    }

    # A hand-written impl bypasses the derive check entirely.
    foreach ($trait in @('Debug', 'Clone', 'Serialize')) {
      $implPattern = "(?m)^\s*impl\s+(?:[A-Za-z0-9_:]*::)?$trait\s+for\s+$escaped(?![A-Za-z0-9_])"
      foreach ($match in [regex]::Matches($Source, $implPattern)) {
        $line = Get-LineNumber -Text $Source -Index $match.Index
        $violations.Add(
          "[secret-derive] ${RelativePath}:$line secret type '$typeName' must not implement $trait"
        )
      }
    }
  }
  return $violations.ToArray()
}

function Find-SecretSinkViolations {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Source,
    [Parameter(Mandatory = $true)][string]$RelativePath
  )

  $violations = New-Object System.Collections.Generic.List[string]
  $accessorAlternation = ($script:SecretAccessors | ForEach-Object { [regex]::Escape($_) }) -join '|'

  # Logging, formatting, and serialization sinks: a secret reaching any of these
  # lands in a log file, an error string, or a payload. Never allowed anywhere.
  $sinks = 'trace|debug|info|warn|error|println|eprintln|print|eprint|format|write|writeln|panic|todo|unimplemented'

  # Assertion macros are a sink too, because a failing assertion renders both
  # operands into the panic message. But that only leaks a real credential in
  # production; a test file's operands are synthetic fixtures, and forbidding
  # them there would mean the secret accessors could not be tested at all.
  $isTestPath = $RelativePath -cmatch '(^|/)tests/'
  if (-not $isTestPath) {
    $sinks = "$sinks|assert|assert_eq|assert_ne|debug_assert|debug_assert_eq|debug_assert_ne"
  }

  $pattern = "(?<![A-Za-z0-9_])(?<sink>$sinks)\s*!\s*[\(\[\{][^\r\n]*(?<![A-Za-z0-9_])(?<accessor>$accessorAlternation)\s*\("
  foreach ($match in [regex]::Matches($Source, $pattern)) {
    $line = Get-LineNumber -Text $Source -Index $match.Index
    $sink = $match.Groups['sink'].Value
    $accessor = $match.Groups['accessor'].Value
    $violations.Add(
      "[secret-sink] ${RelativePath}:$line '$accessor' must not be passed to $sink!"
    )
  }
  return $violations.ToArray()
}

function Find-PlaintextArtifactViolations {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Source,
    [Parameter(Mandatory = $true)][string]$RelativePath
  )

  $violations = New-Object System.Collections.Generic.List[string]
  # A decrypted volume must exist only as a Read+Seek view. Anything that can
  # create a file is a plaintext-copy risk on this crate's paths.
  $writeApis = @(
    'File::create',
    'File::create_new',
    'fs::write',
    'fs::copy',
    'tempfile',
    'NamedTempFile',
    'OpenOptions'
  )
  foreach ($api in $writeApis) {
    $escaped = [regex]::Escape($api)
    foreach ($match in [regex]::Matches($Source, "(?<![A-Za-z0-9_])$escaped(?![A-Za-z0-9_])")) {
      $line = Get-LineNumber -Text $Source -Index $match.Index
      $violations.Add(
        "[plaintext-artifact] ${RelativePath}:$line '$api' cannot be used: the decrypted volume must never be materialized"
      )
    }
  }
  return $violations.ToArray()
}

function Find-UnsafeForbidViolations {
  param(
    [Parameter(Mandatory = $true)][string]$CrateRoot,
    [Parameter(Mandatory = $true)][string]$RelativeCrateRoot
  )

  $violations = New-Object System.Collections.Generic.List[string]
  $libPath = Join-Path $CrateRoot 'src/lib.rs'
  if (-not (Test-Path -LiteralPath $libPath -PathType Leaf)) {
    $violations.Add("[missing-lib] $RelativeCrateRoot/src/lib.rs is missing")
    return $violations.ToArray()
  }
  $source = Read-Utf8Text -Path $libPath
  if ($source -cnotmatch '#!\[forbid\(\s*unsafe_code\s*\)\]') {
    $violations.Add(
      "[unsafe-forbid] $RelativeCrateRoot/src/lib.rs must keep #![forbid(unsafe_code)]"
    )
  }
  return $violations.ToArray()
}

function Find-ProvenanceViolations {
  param([Parameter(Mandatory = $true)][string]$Root)

  $violations = New-Object System.Collections.Generic.List[string]
  $decisionPath = Join-Path $Root 'docs/bitlocker-dependency-decision.md'
  if (-not (Test-Path -LiteralPath $decisionPath -PathType Leaf)) {
    $violations.Add('[provenance] docs/bitlocker-dependency-decision.md is missing')
    return $violations.ToArray()
  }
  $decision = Read-Utf8Text -Path $decisionPath
  if ($decision.IndexOf($script:PinnedUpstreamCommit, [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
    $violations.Add(
      "[provenance] docs/bitlocker-dependency-decision.md must record the pinned upstream commit $($script:PinnedUpstreamCommit)"
    )
  }

  $noticePath = Join-Path $Root 'crates/volume-bitlocker/NOTICE'
  if (-not (Test-Path -LiteralPath $noticePath -PathType Leaf)) {
    $violations.Add('[attribution] crates/volume-bitlocker/NOTICE is missing (Apache-2.0 section 4)')
  } else {
    $notice = Read-Utf8Text -Path $noticePath
    if ($notice.IndexOf($script:PinnedUpstreamCommit, [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
      $violations.Add(
        '[attribution] crates/volume-bitlocker/NOTICE must record the pinned upstream commit'
      )
    }
  }

  $licensePath = Join-Path $Root 'crates/volume-bitlocker/LICENSE-APACHE-2.0-UPSTREAM'
  if (-not (Test-Path -LiteralPath $licensePath -PathType Leaf)) {
    $violations.Add(
      '[attribution] crates/volume-bitlocker/LICENSE-APACHE-2.0-UPSTREAM is missing (Apache-2.0 section 4)'
    )
  }
  return $violations.ToArray()
}

function Find-SerializedSecretContractViolations {
  param([Parameter(Mandatory = $true)][string]$Root)

  $violations = New-Object System.Collections.Generic.List[string]
  $dtoRoot = Join-Path $Root 'crates/transport/src/dto'
  if (-not (Test-Path -LiteralPath $dtoRoot -PathType Container)) {
    return $violations.ToArray()
  }
  $fieldPattern = '(?m)^\s*pub\s+(?<field>credential|passphrase|password|recovery_password|fvek|tweak|volume_key|key_package|persisted_key_blob|key_material)\s*:'
  foreach ($file in Get-ChildItem -LiteralPath $dtoRoot -Recurse -File -Filter '*.rs') {
    $relative = $file.FullName.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
    $source = Read-Utf8Text -Path $file.FullName
    foreach ($match in [regex]::Matches($source, $fieldPattern)) {
      $line = Get-LineNumber -Text $source -Index $match.Index
      $violations.Add(
        "[secret-dto] ${relative}:$line transport DTO field '$($match.Groups['field'].Value)' cannot carry a BitLocker credential"
      )
    }
  }
  return $violations.ToArray()
}

function Find-BitLockerCredentialViolations {
  param([Parameter(Mandatory = $true)][string]$Root)

  $violations = New-Object System.Collections.Generic.List[string]
  $crateRoot = Join-Path $Root 'crates/volume-bitlocker'
  if (-not (Test-Path -LiteralPath $crateRoot -PathType Container)) {
    # The crate is the subject of the guard; without it there is nothing to check
    # and nothing to hide. Provenance still applies once the crate exists.
    return $violations.ToArray()
  }

  foreach ($violation in Find-UnsafeForbidViolations -CrateRoot $crateRoot -RelativeCrateRoot 'crates/volume-bitlocker') {
    $violations.Add($violation)
  }

  $sourceFiles = @(
    Get-ChildItem -LiteralPath $crateRoot -Recurse -File -Filter '*.rs' |
      Sort-Object -Property FullName
  )
  foreach ($file in $sourceFiles) {
    $relative = $file.FullName.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
    $source = Read-Utf8Text -Path $file.FullName
    foreach ($violation in Find-SecretDeriveViolations -Source $source -RelativePath $relative) {
      $violations.Add($violation)
    }
    foreach ($violation in Find-SecretSinkViolations -Source $source -RelativePath $relative) {
      $violations.Add($violation)
    }
    foreach ($violation in Find-PlaintextArtifactViolations -Source $source -RelativePath $relative) {
      $violations.Add($violation)
    }
  }

  foreach ($violation in Find-ProvenanceViolations -Root $Root) {
    $violations.Add($violation)
  }
  foreach ($violation in Find-SerializedSecretContractViolations -Root $Root) {
    $violations.Add($violation)
  }
  return $violations.ToArray()
}

function New-SelfTestCrate {
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$SecretSource
  )

  $src = Join-Path $Root 'crates/volume-bitlocker/src'
  [void](New-Item -ItemType Directory -Path $src -Force)
  $utf8 = [System.Text.UTF8Encoding]::new($false)
  [System.IO.File]::WriteAllText(
    (Join-Path $src 'lib.rs'),
    "#![forbid(unsafe_code)]`nmod secret;`n",
    $utf8
  )
  [System.IO.File]::WriteAllText((Join-Path $src 'secret.rs'), $SecretSource, $utf8)

  $crateRoot = Join-Path $Root 'crates/volume-bitlocker'
  [System.IO.File]::WriteAllText(
    (Join-Path $crateRoot 'NOTICE'),
    "derived from $($script:PinnedUpstreamCommit)`n",
    $utf8
  )
  [System.IO.File]::WriteAllText(
    (Join-Path $crateRoot 'LICENSE-APACHE-2.0-UPSTREAM'),
    "Apache License 2.0`n",
    $utf8
  )
  $docs = Join-Path $Root 'docs'
  [void](New-Item -ItemType Directory -Path $docs -Force)
  [System.IO.File]::WriteAllText(
    (Join-Path $docs 'bitlocker-dependency-decision.md'),
    "Commit: $($script:PinnedUpstreamCommit)`n",
    $utf8
  )
}

function Invoke-SelfTest {
  $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("bitlocker-credential-guard-" + [guid]::NewGuid())
  [void](New-Item -ItemType Directory -Path $temp)
  try {
    $validSecret = @'
use zeroize::Zeroizing;

pub struct Passphrase {
    inner: Zeroizing<String>,
}

impl Passphrase {
    pub fn expose_for_derivation(&self) -> &str {
        &self.inner
    }
}

pub struct VolumeKeyPackage {
    fvek: Zeroizing<Vec<u8>>,
}

impl VolumeKeyPackage {
    pub fn expose_fvek(&self) -> &[u8] {
        &self.fvek
    }
}

pub struct PersistedKeyBlob {
    inner: Zeroizing<Vec<u8>>,
}

impl PersistedKeyBlob {
    pub fn expose_for_storage(&self) -> &[u8] {
        &self.inner
    }
}
'@
    New-SelfTestCrate -Root $temp -SecretSource $validSecret
    $valid = @(Find-BitLockerCredentialViolations -Root $temp)
    if ($valid.Count -ne 0) {
      throw "BitLocker credential guard self-test rejected valid input: $($valid -join '; ')"
    }

    New-SelfTestCrate -Root $temp -SecretSource @'
#[derive(Debug)]
pub struct Passphrase {
    inner: String,
}
'@
    $derived = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($derived -match '^\[secret-derive\].*must not derive Debug')) {
      throw 'Self-test did not reject a secret type deriving Debug'
    }

    New-SelfTestCrate -Root $temp -SecretSource @'
pub struct VolumeKeyPackage {
    fvek: Vec<u8>,
}

impl Clone for VolumeKeyPackage {
    fn clone(&self) -> Self {
        Self { fvek: self.fvek.clone() }
    }
}
'@
    $handWritten = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($handWritten -match '^\[secret-derive\].*must not implement Clone')) {
      throw 'Self-test did not reject a hand-written Clone impl on a secret type'
    }

    New-SelfTestCrate -Root $temp -SecretSource @'
pub struct Passphrase {
    inner: String,
}

impl Passphrase {
    pub fn expose_for_derivation(&self) -> &str {
        &self.inner
    }

    pub fn log_it(&self) {
        tracing::debug!("credential: {}", self.expose_for_derivation());
    }
}
'@
    $sink = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($sink -match '^\[secret-sink\]')) {
      throw 'Self-test did not reject a secret accessor routed into a logging macro'
    }

    New-SelfTestCrate -Root $temp -SecretSource @'
pub struct Passphrase {
    inner: String,
}

impl Passphrase {
    pub fn expose_for_derivation(&self) -> &str {
        &self.inner
    }

    pub fn check(&self) {
        assert_eq!(self.expose_for_derivation(), "expected");
    }
}
'@
    $productionAssert = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($productionAssert -match '^\[secret-sink\].*assert_eq')) {
      throw 'Self-test did not reject a secret accessor inside a production assertion'
    }

    # The same assertion in a test file is fixture data, not a credential, and
    # must stay allowed or the accessors become untestable.
    New-SelfTestCrate -Root $temp -SecretSource $validSecret
    $testDir = Join-Path $temp 'crates/volume-bitlocker/tests/unit'
    [void](New-Item -ItemType Directory -Path $testDir -Force)
    [System.IO.File]::WriteAllText(
      (Join-Path $testDir 'secret.rs'),
      "use super::*;`n#[test]`nfn t() { assert_eq!(p.expose_for_derivation(), `"x`"); }`n",
      [System.Text.UTF8Encoding]::new($false)
    )
    $testAssert = @(Find-BitLockerCredentialViolations -Root $temp)
    if ($testAssert.Count -ne 0) {
      throw "Self-test rejected an assertion in a test file: $($testAssert -join '; ')"
    }
    Remove-Item -LiteralPath (Join-Path $temp 'crates/volume-bitlocker/tests') -Recurse -Force

    New-SelfTestCrate -Root $temp -SecretSource @'
pub fn dump(plaintext: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create("volume.plain")?;
    std::io::Write::write_all(&mut file, plaintext)
}
'@
    $plaintext = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($plaintext -match '^\[plaintext-artifact\]')) {
      throw 'Self-test did not reject plaintext volume materialization'
    }

    New-SelfTestCrate -Root $temp -SecretSource $validSecret
    [System.IO.File]::WriteAllText(
      (Join-Path $temp 'crates/volume-bitlocker/src/lib.rs'),
      "mod secret;`n",
      [System.Text.UTF8Encoding]::new($false)
    )
    $unsafeForbid = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($unsafeForbid -match '^\[unsafe-forbid\]')) {
      throw 'Self-test did not reject a crate that dropped #![forbid(unsafe_code)]'
    }

    New-SelfTestCrate -Root $temp -SecretSource $validSecret
    [System.IO.File]::WriteAllText(
      (Join-Path $temp 'docs/bitlocker-dependency-decision.md'),
      "Commit: 0000000000000000000000000000000000000000`n",
      [System.Text.UTF8Encoding]::new($false)
    )
    $provenance = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($provenance -match '^\[provenance\]')) {
      throw 'Self-test did not reject a drifted upstream commit record'
    }

    New-SelfTestCrate -Root $temp -SecretSource $validSecret
    Remove-Item -LiteralPath (Join-Path $temp 'crates/volume-bitlocker/NOTICE') -Force
    $attribution = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($attribution -match '^\[attribution\]')) {
      throw 'Self-test did not reject a missing NOTICE file'
    }

    New-SelfTestCrate -Root $temp -SecretSource $validSecret
    $dtoRoot = Join-Path $temp 'crates/transport/src/dto'
    [void](New-Item -ItemType Directory -Path $dtoRoot -Force)
    [System.IO.File]::WriteAllText(
      (Join-Path $dtoRoot 'bitlocker.rs'),
      "pub struct RequestDto {`n    pub credential: String,`n    pub key_package: Vec<u8>,`n}`n",
      [System.Text.UTF8Encoding]::new($false)
    )
    $secretDto = @(Find-BitLockerCredentialViolations -Root $temp)
    if (-not ($secretDto -match "field 'credential'") -or -not ($secretDto -match "field 'key_package'")) {
      throw 'Self-test did not reject a serializable BitLocker credential field'
    }
  } finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
  }

  Write-Host 'BitLocker credential guard self-test passed.'
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
$violations = @(Find-BitLockerCredentialViolations -Root $resolvedRoot)
if ($violations.Count -gt 0) {
  throw "BitLocker credential guard failed:`n$($violations -join "`n")"
}

Write-Host 'BitLocker credential guard passed.'
