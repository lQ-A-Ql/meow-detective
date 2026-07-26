$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Read-RepoFile([string]$relativePath) {
  $path = Join-Path $repoRoot $relativePath
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required file is missing: $relativePath"
  }
  return Get-Content -LiteralPath $path -Raw -Encoding UTF8
}

function Require-Contains([string]$name, [string]$content, [string]$needle) {
  if (-not $content.Contains($needle)) {
    throw "$name missing required media protocol guard: $needle"
  }
}

function Require-NotContains([string]$name, [string]$content, [string]$needle) {
  if ($content.Contains($needle)) {
    throw "$name contains forbidden media protocol pattern: $needle"
  }
}

$tauriConfig = Read-RepoFile "apps/desktop/src-tauri/tauri.conf.json"
$lib = Read-RepoFile "apps/desktop/src-tauri/src/lib.rs"
$mediaProtocol = Read-RepoFile "apps/desktop/src-tauri/src/media_protocol.rs"
$fileCommands = Read-RepoFile "apps/desktop/src-tauri/src/commands/file_commands/media.rs"
$viewerDto = Read-RepoFile "crates/transport/src/dto/viewer.rs"
$frontendFilesApi = Read-RepoFile "frontend/src/lib/api/files.ts"
$frontendCommands = Read-RepoFile "frontend/src/lib/api/commands.ts"
$frontendFilesHooks = Read-RepoFile "frontend/src/features/files/hooks.ts"

Require-Contains "tauri.conf.json" $tauriConfig "media-src 'self' data: evidence-media:"
Require-Contains "lib.rs" $lib "mod media_protocol;"
Require-Contains "lib.rs" $lib "media_protocol::register(tauri::Builder::default())"
Require-Contains "media_protocol.rs" $mediaProtocol "pub const EVIDENCE_MEDIA_SCHEME: &str = `"evidence-media`";"
Require-Contains "media_protocol.rs" $mediaProtocol "register_asynchronous_uri_scheme_protocol"
Require-Contains "media_protocol.rs" $mediaProtocol "MAX_MEDIA_PROTOCOL_READ_BYTES"
Require-Contains "media_protocol.rs" $mediaProtocol "parse_media_range_header"
Require-Contains "media_protocol.rs" $mediaProtocol "StatusCode::PARTIAL_CONTENT"
Require-Contains "media_protocol.rs" $mediaProtocol "StatusCode::RANGE_NOT_SATISFIABLE"
Require-Contains "file_commands/media.rs" $fileCommands "MediaPreviewModeDto::Protocol"
Require-Contains "file_commands/media.rs" $fileCommands "media_protocol::media_protocol_url"
Require-Contains "file_commands/media.rs" $fileCommands "pub async fn read_media_range"
Require-Contains "viewer.rs" $viewerDto "pub const MAX_VIEWER_RANGE_LENGTH: u32 = 1024 * 1024;"
Require-Contains "commands.ts" $frontendCommands "GET_MEDIA_URL: 'get_media_url'"
Require-Contains "commands.ts" $frontendCommands "READ_MEDIA_RANGE: 'read_media_range'"
Require-Contains "files.ts" $frontendFilesApi "COMMANDS.files.GET_MEDIA_URL"
Require-Contains "files.ts" $frontendFilesApi "COMMANDS.files.READ_MEDIA_RANGE"
Require-Contains "hooks.ts" $frontendFilesHooks "media.mode === 'protocol'"
Require-Contains "hooks.ts" $frontendFilesHooks "previewMode: 'rangeFallback'"

$scanRoots = @(
  "apps/desktop/src-tauri",
  "frontend/src/lib/api",
  "frontend/src/features/files",
  "frontend/src/app/pages/FileBrowser.tsx"
)

foreach ($root in $scanRoots) {
  $fullRoot = Join-Path $repoRoot $root
  if (-not (Test-Path -LiteralPath $fullRoot)) {
    continue
  }
  Get-ChildItem -LiteralPath $fullRoot -Recurse -File |
    Where-Object { $_.Extension -in @(".rs", ".ts", ".tsx", ".json") } |
    ForEach-Object {
      $content = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8
      $relative = $_.FullName.Substring($repoRoot.Path.Length).TrimStart('\', '/')
      Require-NotContains $relative $content "asset://localhost"
      Require-NotContains $relative $content "convertFileSrc"
    }
}

Write-Host "Media protocol guard passed"
