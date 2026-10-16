Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$repo=(Resolve-Path "$PSScriptRoot/../..").Path
Import-Module "$repo/scripts/lib/ImportBenchmark.Process.psm1" -Force
$directory=Join-Path $repo ('target/process-protocol-'+[guid]::NewGuid().ToString('N'))
$null=New-Item -ItemType Directory -Path $directory
$exe=(Get-Command powershell.exe).Source
$checks=0
foreach ($scenario in @(
    @{name='success';args=@('-NoProfile','-NonInteractive','-Command','exit','0');timeout=10;status='completed';exit=0},
    @{name='failure';args=@('-NoProfile','-NonInteractive','-Command','exit','7');timeout=10;status='failed';exit=7},
    @{name='timeout';args=@('-NoProfile','-NonInteractive','-Command','Start-Sleep','-Seconds','5');timeout=1;status='timeout';exit=$null}
  )) {
  $log=Join-Path $directory ($scenario.name+'.log'); $threw=$false
  try { $null=Invoke-ImportBenchmarkProcess -Executable $exe -Arguments $scenario.args -WorkingDirectory $repo -Log $log -TimeoutSeconds $scenario.timeout } catch { $threw=$true }
  if ($threw -ne ($scenario.status -ne 'completed')) { throw 'Wrong failure propagation' }; $checks++
  $record=Get-Content -LiteralPath "$log.process.json" -Raw | ConvertFrom-Json
  if ($record.status -ne $scenario.status -or -not (Test-Path -LiteralPath $log)) { throw 'Missing durable failure output' }; $checks++
  if ($null -ne $scenario.exit -and $record.exitCode -ne $scenario.exit) { throw 'Wrong exit code' }; $checks++
}
$raw=Join-Path $directory 'unused.raw'; [IO.File]::WriteAllBytes($raw,(New-Object byte[] 4096))
$launchLog=Join-Path $directory 'launch-error.log'; $failed=$false
try { $null=Invoke-ImportBenchmarkProcess -Executable "$directory/absent.exe" -Arguments @() -WorkingDirectory $repo -Log $launchLog } catch { $failed=$true }
if (-not $failed -or (Get-Content -LiteralPath "$launchLog.process.json" -Raw | ConvertFrom-Json).status -ne 'process-error') { throw 'Launch error lost diagnostics' }; $checks++
$definition=@{schemaVersion=1; versions=@(@{name='failure';executable=$exe;sha256=(Get-FileHash $exe).Hash;revision='failure-test';analysisModes=@('metadataOnly')})
  inputs=@(@{name='unused';path=$raw;logicalGroup='unused'})}
$definitionPath=Join-Path $directory 'definition.json'; $definition | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $definitionPath -Encoding UTF8
$failed=$false
try { & "$repo/scripts/run-import-comparison.ps1" -DefinitionPath $definitionPath -OutputDir "$directory/matrix" -RunKind Exploratory -Runs 1 -WarmupRuns 0 -Workers 4 } catch { $failed=$true }
if (-not $failed) {throw 'Matrix process error must propagate'}; $checks++
$reportFile=Get-ChildItem -LiteralPath "$directory/matrix" -Filter report.json -Recurse
$report=Get-Content -LiteralPath $reportFile.FullName -Raw | ConvertFrom-Json
if ($report.status -ne 'failed' -or $report.trials.Count -ne 1 -or $report.trials[0].status -ne 'failed' -or $report.summary.Count) {throw 'Failed trial must be durable and excluded from summary'}; $checks++
if (-not (Test-Path -LiteralPath ($report.trials[0].log+'.process.json'))) {throw 'Matrix lost raw process failure'}; $checks++
Write-Host "Process protocol: $checks assertions passed; evidence $directory"
