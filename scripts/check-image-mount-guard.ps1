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
$dokanLifecycle = Read-RepoFile 'apps/desktop/src-tauri/src/mount_backend/dokan/lifecycle.rs'
$mountRegistry = Read-RepoFile 'apps/desktop/src-tauri/src/mount_registry.rs'
if (-not [regex]::IsMatch($backendMod, '(?s)#\[cfg\(windows\)\]\s*mod dokan;')) {
  $errors.Add('Dokan module is not guarded by cfg(windows)')
}
if (-not $backendMod.Contains('#[cfg(not(windows))]')) {
  $errors.Add('non-Windows mount backend must fail closed with an explicit unsupported branch')
}
Add-ForbiddenMatches 'apps/desktop/src-tauri/src/mount_backend/mod.rs' $backendMod
Add-ForbiddenMatches 'apps/desktop/src-tauri/src/mount_backend/dokan.rs' $dokan
Add-ForbiddenMatches 'apps/desktop/src-tauri/src/mount_backend/dokan/lifecycle.rs' $dokanLifecycle
foreach ($required in @(
  'MountFlags::WRITE_PROTECT',
  'MountFlags::MOUNT_MANAGER',
  'StartupEvent::Mounted',
  'pub(crate) fn poll_exit'
)) {
  if (-not $dokanLifecycle.Contains($required)) {
    $errors.Add("logical Dokan mount publication is missing required contract: $required")
  }
}
if ($dokanLifecycle.Contains('MountFlags::CURRENT_SESSION')) {
  $errors.Add('elevated logical mounts must not be isolated to the administrator session')
}
if (-not $dokan.Contains('self.publication.publish(mount_point)')) {
  $errors.Add('logical mount readiness must come from the Dokan Mounted callback')
}
if (-not $mountRegistry.Contains('refresh_backend_state')) {
  $errors.Add('logical mount registry must detect an exited Dokan worker')
}

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
$windowsService = Read-RepoFile 'crates/physical-mount/src/windows_service.rs'
foreach ($required in @(
  'QueryServiceConfigW',
  'ChangeServiceConfigW',
  'SERVICE_DISABLED',
  'SERVICE_DEMAND_START',
  'IscsiServiceLease',
  'restore_start_type_if_owned'
)) {
  if (-not $windowsService.Contains($required)) {
    $errors.Add("Windows iSCSI service lifecycle is missing required contract: $required")
  }
}
if ($windowsService.Contains('ControlService')) {
  $errors.Add('physical mount must not stop the shared Microsoft iSCSI service')
}
$tauriManifest = Read-RepoFile 'apps/desktop/src-tauri/windows-app-manifest.xml'
$tauriBuild = Read-RepoFile 'apps/desktop/src-tauri/build.rs'
$tauriManifestResource = Read-RepoFile 'apps/desktop/src-tauri/windows-app-manifest.rc'
$tauriTestManifest = Read-RepoFile 'apps/desktop/src-tauri/windows-test-manifest.xml'
$tauriTestManifestResource = Read-RepoFile 'apps/desktop/src-tauri/windows-test-manifest.rc'
foreach ($required in @(
  'requestedExecutionLevel level="requireAdministrator"',
  'uiAccess="false"',
  'Microsoft.Windows.Common-Controls'
)) {
  if (-not $tauriManifest.Contains($required)) {
    $errors.Add("Windows application manifest is missing required entry: $required")
  }
}
foreach ($required in @(
  'WindowsAttributes::new_without_app_manifest',
  'embed_resource::compile_for',
  'embed_resource::compile_for_tests',
  '["forensics-desktop"]'
)) {
  if (-not $tauriBuild.Contains($required)) {
    $errors.Add("administrator manifest is not isolated to the desktop binary: $required")
  }
}
if (-not $tauriManifestResource.Contains('1 RT_MANIFEST "windows-app-manifest.xml"')) {
  $errors.Add('Windows administrator manifest resource binding is missing')
}
if (-not $tauriTestManifest.Contains('Microsoft.Windows.Common-Controls') -or
    $tauriTestManifest.Contains('requireAdministrator')) {
  $errors.Add('Windows test manifest must provide Common Controls without elevation')
}
if (-not $tauriTestManifestResource.Contains('1 RT_MANIFEST "windows-test-manifest.xml"')) {
  $errors.Add('Windows non-elevated test manifest resource binding is missing')
}
$desktopUnitTests = Read-RepoFile 'apps/desktop/src-tauri/tests/unit/lib.rs'
if (-not $desktopUnitTests.Contains('link(name = "windows-test-manifest"') -or
    $desktopUnitTests.Contains('link(name = "resource"')) {
  $errors.Add('desktop unit tests must link only the non-elevated test manifest')
}
$desktopManifest = Read-RepoFile 'apps/desktop/src-tauri/Cargo.toml'
foreach ($required in @('name = "forensics-desktop"', 'test = false', 'bench = false')) {
  if (-not $desktopManifest.Contains($required)) {
    $errors.Add("administrator desktop binary must not create a test harness: $required")
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
