param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$mcpCommandsPath = Join-Path $repoRoot "apps/desktop/src-tauri/src/commands/mcp_commands.rs"
$mcpDtoPath = Join-Path $repoRoot "crates/transport/src/dto/mcp.rs"
$stagingDir = Join-Path $repoRoot "crates/app-services/src/staging"

foreach ($path in @($mcpCommandsPath, $mcpDtoPath, $stagingDir)) {
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required Stage 5 regression guard input is missing: $path"
  }
}

$mcpCommands = Get-Content -LiteralPath $mcpCommandsPath -Raw -Encoding UTF8
$mcpDto = Get-Content -LiteralPath $mcpDtoPath -Raw -Encoding UTF8
$staging = (Get-ChildItem -LiteralPath $stagingDir -Filter '*.rs' -File | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 }) -join "`n"

function Assert-Matches {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Content -notmatch $Pattern) {
    throw $Message
  }
}

function Assert-NotMatches {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Content -match $Pattern) {
    throw $Message
  }
}

# MCP transport validation must reject unknown transports everywhere the command
# layer converts DTOs into concrete MCP transports. In particular, save_mcp_config
# must not silently default an invalid transport to an empty SSE endpoint.
Assert-NotMatches `
  -Content $mcpCommands `
  -Pattern '_\s*=>\s*McpTransport::Sse\s*\{\s*url:\s*String::new\(\)\s*\}' `
  -Message "MCP save/config conversion must not silently default invalid transports to empty SSE"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'fn\s+transport_from_dto[\s\S]{0,800}_\s*=>\s*Err\([\s\S]{0,160}Invalid transport type' `
  -Message "transport_from_dto must reject invalid MCP transport types"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'pub\s+async\s+fn\s+save_mcp_config[\s\S]{0,700}config_from_dto\(config\)\?' `
  -Message "save_mcp_config must use validating MCP config conversion"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'pub\s+async\s+fn\s+add_mcp_server[\s\S]{0,700}server_config_from_dto\(&server\)\?' `
  -Message "add_mcp_server must use validating MCP server conversion"

Assert-Matches `
  -Content $mcpCommands `
  -Pattern 'pub\s+async\s+fn\s+test_mcp_connection[\s\S]{0,700}Invalid transport type' `
  -Message "test_mcp_connection must validate and report invalid MCP transport types"

foreach ($pattern in @(
    '"sse"\s*=>\s*McpTransport::Sse',
    '"stdio"\s*=>\s*McpTransport::Stdio'
  )) {
  Assert-Matches `
    -Content $mcpCommands `
    -Pattern $pattern `
    -Message "MCP command transport conversion is missing expected arm: $pattern"
}

# Transport DTOs currently serialize as snake_case nested payloads. Guard this
# boundary because the frontend/Tauri command envelope is camelCase, while these
# nested MCP DTOs are not.
foreach ($pattern in @(
    'server_config_serializes_current_snake_case_response_fields',
    'tool_call_request_documents_camel_case_boundary_is_top_level_only',
    'transport_type:\s*String',
    'server_id:\s*String',
    'tool_name:\s*String'
  )) {
  Assert-Matches `
    -Content $mcpDto `
    -Pattern $pattern `
    -Message "MCP transport DTO regression check is missing expected contract marker: $pattern"
}

# Staging merges must not use INSERT OR IGNORE in the merge-to-main path. Silent
# conflict suppression hides missing/duplicate rows and lets staging partitions be
# marked merged even when rows were skipped.
foreach ($pattern in @(
    'INSERT\s+OR\s+IGNORE\s+INTO\s+main\.file_entries',
    'INSERT\s+OR\s+IGNORE\s+INTO\s+main\.artifacts',
    'INSERT\s+OR\s+IGNORE\s+INTO\s+main\.timeline_events'
  )) {
  Assert-NotMatches `
    -Content $staging `
    -Pattern $pattern `
    -Message "Staging merge-to-main must not silently suppress conflicts with: $pattern"
}

foreach ($pattern in @(
    'INSERT\s+INTO\s+main\.file_entries',
    'INSERT\s+INTO\s+main\.artifacts',
    'INSERT\s+INTO\s+main\.timeline_events'
  )) {
  Assert-Matches `
    -Content $staging `
    -Pattern $pattern `
    -Message "Staging merge-to-main is missing explicit insert path: $pattern"
}

Write-Host "Stage 5 regression guard passed: MCP transport validation, nested MCP DTO contract, and staging conflict visibility are locked"
