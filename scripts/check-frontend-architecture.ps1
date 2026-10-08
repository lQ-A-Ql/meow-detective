[CmdletBinding()]
param([switch]$SelfTest)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$guardPath = Join-Path $PSScriptRoot 'lib/FrontendArchitectureGuard.mjs'
if (-not (Test-Path -LiteralPath $guardPath)) { throw "Frontend architecture analyzer is missing: $guardPath" }
if (-not (Get-Command node -ErrorAction SilentlyContinue)) { throw 'Node.js is required by the TypeScript architecture analyzer.' }
$arguments = @($guardPath, '--root', $repoRoot)
if ($SelfTest) { $arguments += '--self-test' }
& node @arguments
if ($LASTEXITCODE -ne 0) { throw "Frontend architecture guard failed (exit $LASTEXITCODE)." }
