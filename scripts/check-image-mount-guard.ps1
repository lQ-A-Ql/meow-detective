param(
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$errors = New-Object System.Collections.Generic.List[string]

function Read-RepoFile([string]$RelativePath) {
  $path = Join-Path $repoRoot $RelativePath
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    $errors.Add("missing mount file: $RelativePath")
    return ''
  }
  return [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
}

function Add-ForbiddenMatches([string]$RelativePath, [string]$Content) {
  foreach ($pattern in @(
    'std::process::Command',
    'Command::new',
    'std::fs::write',
    'std::fs::rename',
    'std::fs::remove_file',
    'std::fs::remove_dir',
    'ewfmount',
    'qemu-nbd',
    'rbd map',
    'mount.ceph',
    'ceph-fuse'
  )) {
    if ($Content.Contains($pattern)) {
      $errors.Add("mount production code contains forbidden host operation '$pattern': $RelativePath")
    }
  }
}

$evidenceMountRoot = Join-Path $repoRoot 'crates/evidence-mount/src'
foreach ($coreRoot in @($evidenceMountRoot, (Join-Path $repoRoot 'crates/evidence-block/src'))) {
  if (Test-Path -LiteralPath $coreRoot -PathType Container) {
    foreach ($file in Get-ChildItem -LiteralPath $coreRoot -Recurse -File -Filter '*.rs') {
      $relative = $file.FullName.Substring($repoRoot.Length + 1).Replace('\', '/')
      $content = [System.IO.File]::ReadAllText($file.FullName, [System.Text.Encoding]::UTF8)
      if ($content -match '(?i)tauri|@tauri-apps|sqlite|rusqlite') {
        $errors.Add("evidence-mount core crosses an application/runtime boundary: $relative")
      }
      Add-ForbiddenMatches $relative $content
    }
  }
}

$backendMod = Read-RepoFile 'apps/desktop/src-tauri/src/mount_backend/mod.rs'
$dokan = Read-RepoFile 'apps/desktop/src-tauri/src/mount_backend/dokan.rs'
if (-not [regex]::IsMatch($backendMod, '(?s)#\[cfg\(windows\)\]\s*mod dokan;')) {
  $errors.Add('Dokan module is not guarded by cfg(windows)')
}
if (-not $backendMod.Contains('#[cfg(not(windows))]')) {
  $errors.Add('non-Windows mount backend must fail closed with an explicit unsupported branch')
}
Add-ForbiddenMatches 'apps/desktop/src-tauri/src/mount_backend/mod.rs' $backendMod
Add-ForbiddenMatches 'apps/desktop/src-tauri/src/mount_backend/dokan.rs' $dokan

$physicalRoot = Join-Path $repoRoot 'crates/physical-mount/src'
if (-not (Test-Path -LiteralPath $physicalRoot -PathType Container)) {
  $errors.Add('physical-mount crate is missing')
} else {
  foreach ($file in Get-ChildItem -LiteralPath $physicalRoot -Recurse -File -Filter '*.rs') {
    $relative = $file.FullName.Substring($repoRoot.Length + 1).Replace('\', '/')
    $content = [System.IO.File]::ReadAllText($file.FullName, [System.Text.Encoding]::UTF8)
    Add-ForbiddenMatches $relative $content
    if ($content.Contains('0.0.0.0')) {
      $errors.Add("physical mount must not bind a non-loopback listener: $relative")
    }
  }
}

$physicalTarget = Read-RepoFile 'crates/physical-mount/src/target.rs'
if (-not $physicalTarget.Contains('TcpListener::bind(("127.0.0.1", 0))')) {
  $errors.Add('physical iSCSI target is not bound explicitly to loopback')
}
$windowsInitiator = Read-RepoFile 'crates/physical-mount/src/windows_initiator.rs'
foreach ($required in @('LoginIScsiTargetW', 'LogoutIScsiTarget', 'GetDevicesForIScsiSessionW')) {
  if (-not $windowsInitiator.Contains($required)) {
    $errors.Add("Windows physical mount is missing required API: $required")
  }
}
$tauriManifest = Read-RepoFile 'apps/desktop/src-tauri/windows-app-manifest.xml'
foreach ($required in @(
  'requestedExecutionLevel level="requireAdministrator"',
  'uiAccess="false"',
  'Microsoft.Windows.Common-Controls'
)) {
  if (-not $tauriManifest.Contains($required)) {
    $errors.Add("Windows application manifest is missing required entry: $required")
  }
}
$patchedScsi = Read-RepoFile 'vendor/iscsi-target/src/scsi.rs'
foreach ($required in @('is_read_only', 'SenseData::write_protected()', '0x80')) {
  if (-not $patchedScsi.Contains($required)) {
    $errors.Add("patched iSCSI read-only contract is missing: $required")
  }
}

$request = Read-RepoFile 'crates/transport/src/dto/mount.rs'
if ($request -match '(?i)password|recovery.?key|vmk|fvek|source_path|source_db') {
  $errors.Add('mount DTO exposes evidence paths or decryption material')
}

if ($SelfTest) {
  if ($errors.Count -gt 0) {
    throw "image mount guard self-test failed:`n$($errors -join "`n")"
  }
  Write-Host 'Image mount guard self-test passed'
  exit 0
}

if ($errors.Count -gt 0) {
  throw "image mount guard failed:`n$($errors -join "`n")"
}

Write-Host 'Image mount guard passed'
