param()

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$strictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)
$errors = New-Object System.Collections.Generic.List[string]

function Read-StrictUtf8([string]$Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
    throw "UTF-8 BOM is not allowed: $Path"
  }
  return $strictUtf8.GetString($bytes)
}

function Count-Lines([string]$Content) {
  if ($Content.Length -eq 0) {
    return 0
  }
  return ($Content -split "`r?`n").Count
}

$facades = @(
  'crates/transport/src/commands/mod.rs',
  'apps/desktop/src-tauri/src/commands/file_commands.rs',
  'apps/desktop/src-tauri/src/commands/case_commands.rs',
  'apps/desktop/src-tauri/src/commands/mcp_commands.rs',
  'apps/desktop/src-tauri/src/commands/analysis_commands.rs',
  'apps/desktop/src-tauri/src/commands/batch_commands.rs'
)

foreach ($relativePath in $facades) {
  $absolutePath = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    $errors.Add("missing Stage 3 facade: $relativePath")
    continue
  }
  $content = Read-StrictUtf8 $absolutePath
  $lineCount = Count-Lines $content
  if ($lineCount -gt 200) {
    $errors.Add("Stage 3 facade exceeds 200 lines: $relativePath ($lineCount)")
  }
}

$transportRoot = Join-Path $repoRoot 'crates/transport/src/commands'
$transportMod = Join-Path $transportRoot 'mod.rs'
if (Test-Path -LiteralPath $transportMod -PathType Leaf) {
  $content = Read-StrictUtf8 $transportMod
  foreach ($forbidden in @(
    '#[derive(',
    'pub struct ',
    'pub enum ',
    'impl ',
    'fn validate',
    'serde_json::'
  )) {
    if ($content.Contains($forbidden)) {
      $errors.Add("transport command root contains request implementation '$forbidden'")
    }
  }
}

$commandRoot = Join-Path $repoRoot 'apps/desktop/src-tauri/src/commands'
$forbiddenCommandDependencies = @(
  '\bevidence_core::',
  '\bfs_ntfs::',
  '\bfs_ext4::',
  '\bfs_xfs::',
  '\bfs_btrfs::',
  '\bimage_e01::'
)

$approvedRepositoryReferences = @{
  'apps/desktop/src-tauri/src/commands/command_support.rs' = @('audit_repo')
  'apps/desktop/src-tauri/src/commands/file_commands/support.rs' = @('audit_repo')
  'apps/desktop/src-tauri/src/commands/import/cancellation.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/background_job/cluster.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/background_job/cluster_members.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/background_job/cluster_status.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/background_job/gate.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/background_job/single.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/background_job/status.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/schedule/cluster_queue.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/import/schedule/queue.rs' = @('job_repo')
  'apps/desktop/src-tauri/src/commands/mcp_commands/config.rs' = @('audit_repo')
  'apps/desktop/src-tauri/src/commands/mcp_commands/lifecycle.rs' = @('audit_repo')
  'apps/desktop/src-tauri/src/commands/mcp_commands/prompts.rs' = @('audit_repo')
  'apps/desktop/src-tauri/src/commands/mcp_commands/resources.rs' = @('audit_repo')
  'apps/desktop/src-tauri/src/commands/mcp_commands/tools.rs' = @('audit_repo')
}

function Find-UnapprovedRepositoryReference(
  [string]$Content,
  [string]$RelativePath
) {
  $remaining = $Content
  if ($approvedRepositoryReferences.ContainsKey($RelativePath)) {
    foreach ($module in $approvedRepositoryReferences[$RelativePath]) {
      $pattern =
        "persistence_sqlite::repositories::$module::(?:\{[^}]*\}|[A-Za-z_][A-Za-z0-9_]*)"
      $remaining = [regex]::Replace($remaining, $pattern, '')
    }
  }
  return $remaining.Contains('persistence_sqlite::repositories::')
}

foreach ($file in Get-ChildItem -LiteralPath $commandRoot -Recurse -File -Filter '*.rs') {
  $content = Read-StrictUtf8 $file.FullName
  $relative = $file.FullName.Substring($repoRoot.Length + 1).Replace('\', '/')
  $lineCount = Count-Lines $content
  if ($lineCount -gt 200) {
    $errors.Add("desktop command module exceeds 200 lines: $relative ($lineCount)")
  }
  if (Find-UnapprovedRepositoryReference $content $relative) {
    $errors.Add("command directly references an unapproved repository implementation: $relative")
  }
  foreach ($pattern in $forbiddenCommandDependencies) {
    if ([regex]::IsMatch($content, $pattern)) {
      $errors.Add("command directly depends on repository/parser/evidence implementation: $relative ($pattern)")
    }
  }
}

$requiredTransportDomains = @(
  'analysis.rs',
  'case.rs',
  'files.rs',
  'import.rs',
  'timeline.rs'
)

foreach ($fileName in $requiredTransportDomains) {
  $path = Join-Path $transportRoot $fileName
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    $errors.Add("missing transport command domain: crates/transport/src/commands/$fileName")
  }
}

if ($errors.Count -gt 0) {
  Write-Error "Stage 3 command boundary guard failed:`n$($errors -join "`n")"
}

Write-Host 'Stage 3 command boundary guard passed'
