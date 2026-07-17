#Requires -Version 5.1

param(
    [string]$CaseRoot = $env:FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT,
    [string]$NativeXfsFixture = $env:FORENSICS_LINUX_E01_FIXTURE,
    [string]$PveClusterRoot = $env:FORENSICS_PVE_CLUSTER_ROOT,
    [switch]$RequireFixture,
    [switch]$RequireComparisonFixtures,
    [ValidateRange(1, 20)]
    [int]$Runs = 3,
    [ValidateRange(1, 3600)]
    [int]$TimeoutSeconds = 120,
    [ValidateRange(1, 3600)]
    [int]$BuildTimeoutSeconds = 900,
    [double]$MaxWarmSame64KiBP95Ms = 200,
    [double]$MaxSequential16x64KiBP95Ms = 200,
    [double]$MaxSequential4x1MiBMedianP95Ms = 300,
    [double]$MaxLargeRandom64KiBMedianP95Ms = 500,
    [double]$MaxColdFileReadMedianMs = 1500,
    [double]$MaxNativeWarmSame64KiBP95Ms = 50,
    [double]$MaxNativeSequential4x1MiBP95Ms = 100,
    [double]$MaxPveHostWarmSame64KiBP95Ms = 50,
    [double]$MaxPveHostSequential4x1MiBP95Ms = 100,
    [double]$MaxRbdToNativeWarmRatio = 3,
    [double]$WarmRatioNoiseFloorMs = 1,
    [int64]$MaxRuntimeCacheBytes = 134217728,
    [int]$MaxRssDeltaMb = 640,
    [string]$OraclePath = "",
    [string]$OutputDir = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
. (Join-Path $PSScriptRoot "lib/RustGuard.Common.ps1")

$sourceRoot = if ([string]::IsNullOrWhiteSpace($CaseRoot)) {
    ""
} else {
    Join-Path $CaseRoot "sources"
}
$hasSourceDb = -not [string]::IsNullOrWhiteSpace($sourceRoot) -and
    (Test-Path -LiteralPath $sourceRoot -PathType Container) -and
    $null -ne (Get-ChildItem -LiteralPath $sourceRoot -Filter "source.db" -File -Recurse -ErrorAction SilentlyContinue |
        Select-Object -First 1)
if ([string]::IsNullOrWhiteSpace($CaseRoot) -or
    -not (Test-Path -LiteralPath (Join-Path $CaseRoot "app.db") -PathType Leaf) -or
    -not $hasSourceDb) {
    $message = "FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT is not set to a retained case containing app.db and a source.db."
    if ($RequireFixture) {
        throw $message
    }
    Write-Host "SKIP: $message"
    exit 0
}

$resolvedCaseRoot = (Resolve-Path -LiteralPath $CaseRoot).Path
$hasNativeXfsFixture = -not [string]::IsNullOrWhiteSpace($NativeXfsFixture) -and
    (Test-Path -LiteralPath $NativeXfsFixture -PathType Leaf)
$hasPveClusterRoot = -not [string]::IsNullOrWhiteSpace($PveClusterRoot) -and
    (Test-Path -LiteralPath $PveClusterRoot -PathType Container)
if ($RequireComparisonFixtures -and (-not $hasNativeXfsFixture -or -not $hasPveClusterRoot)) {
    throw "Comparison fixtures require FORENSICS_LINUX_E01_FIXTURE and FORENSICS_PVE_CLUSTER_ROOT."
}
$resolvedNativeXfsFixture = if ($hasNativeXfsFixture) {
    (Resolve-Path -LiteralPath $NativeXfsFixture).Path
} else {
    ""
}
$resolvedPveClusterRoot = if ($hasPveClusterRoot) {
    (Resolve-Path -LiteralPath $PveClusterRoot).Path
} else {
    ""
}
if (-not $hasNativeXfsFixture) {
    Write-Host "SKIP: native XFS comparison fixture is unavailable."
}
if (-not $hasPveClusterRoot) {
    Write-Host "SKIP: PVE host EXT4 comparison fixture is unavailable."
}
if ([string]::IsNullOrWhiteSpace($OraclePath)) {
    $OraclePath = Join-Path $projectRoot "testdata/real-samples/pve-rbd-preview-oracle.json"
}
if (-not (Test-Path -LiteralPath $OraclePath -PathType Leaf)) {
    throw "Fixed PVE RBD preview oracle was not found."
}
$fixedOracle = Get-Content -LiteralPath $OraclePath -Raw -Encoding UTF8 | ConvertFrom-Json
if ([int]$fixedOracle.schemaVersion -ne 1) {
    throw "Unsupported fixed PVE RBD preview oracle schema version."
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $projectRoot "artifacts/pve-rbd-preview-performance"
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$testName = "retained_pve_rbd_preview_performance"
$cargo = Get-Command cargo -CommandType Application -ErrorAction Stop | Select-Object -First 1

function Get-Median {
    param([Parameter(Mandatory = $true)][double[]]$Values)

    if ($Values.Count -eq 0) {
        throw "Cannot calculate a median for an empty value set."
    }
    $sorted = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if (($sorted.Count % 2) -eq 1) {
        return [double]$sorted[$middle]
    }
    return ([double]$sorted[$middle - 1] + [double]$sorted[$middle]) / 2.0
}

function Get-Metric {
    param(
        [Parameter(Mandatory = $true)][object]$Report,
        [Parameter(Mandatory = $true)][string]$Scenario
    )

    $matches = @($Report.metrics | Where-Object { $_.scenario -eq $Scenario })
    if ($matches.Count -ne 1) {
        throw "Expected exactly one metric named '$Scenario', found $($matches.Count)."
    }
    return $matches[0]
}

function Get-CaseStorageFingerprint {
    param([Parameter(Mandatory = $true)][string]$Path)

    $normalized = [System.IO.Path]::GetFullPath($Path).ToLowerInvariant()
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($normalized)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($sha256.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    } finally {
        $sha256.Dispose()
    }
}

function Protect-PerformanceLog {
    param([Parameter(Mandatory = $true)][string]$Text)

    $protected = $Text.Replace($resolvedCaseRoot, "<CASE_ROOT>")
    $protected = $protected.Replace($projectRoot, "<PROJECT_ROOT>")
    if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $protected = $protected.Replace($env:USERPROFILE, "<USER_PROFILE>")
    }
    if (-not [string]::IsNullOrWhiteSpace($resolvedNativeXfsFixture)) {
        $protected = $protected.Replace($resolvedNativeXfsFixture, "<NATIVE_XFS_FIXTURE>")
    }
    if (-not [string]::IsNullOrWhiteSpace($resolvedPveClusterRoot)) {
        $protected = $protected.Replace($resolvedPveClusterRoot, "<PVE_CLUSTER_ROOT>")
    }
    return $protected
}

function Invoke-PreviewPerformanceBuild {
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $cargo.Source
    $startInfo.Arguments = (
        "test -p app-services " +
        "--test pve_rbd_preview_performance " +
        "--test native_linux_preview_performance " +
        "--test pve_host_preview_performance --no-run"
    )
    $startInfo.WorkingDirectory = $projectRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $result = Invoke-RustGuardProcess `
        -StartInfo $startInfo `
        -TimeoutMilliseconds ($BuildTimeoutSeconds * 1000) `
        -TimeoutContext "retained PVE RBD preview performance build"
    if ($result.ExitCode -ne 0) {
        throw "PVE RBD preview performance test build failed with exit code $($result.ExitCode)."
    }
}

function Invoke-ComparisonPerformanceRun {
    param(
        [Parameter(Mandatory = $true)][int]$Run,
        [Parameter(Mandatory = $true)][string]$TestTarget,
        [Parameter(Mandatory = $true)][string]$TestName,
        [Parameter(Mandatory = $true)][string]$MetricMarker,
        [Parameter(Mandatory = $true)][string]$EnvironmentName,
        [Parameter(Mandatory = $true)][string]$EnvironmentValue,
        [Parameter(Mandatory = $true)][string]$LogLabel
    )

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $cargo.Source
    $startInfo.Arguments = "test -p app-services --test $TestTarget $TestName -- --ignored --exact --nocapture --test-threads=1"
    $startInfo.WorkingDirectory = $projectRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.Environment[$EnvironmentName] = $EnvironmentValue

    $result = Invoke-RustGuardProcess `
        -StartInfo $startInfo `
        -TimeoutMilliseconds ($TimeoutSeconds * 1000) `
        -TimeoutContext "$LogLabel preview performance run $Run"
    $combined = $result.Stdout + [Environment]::NewLine + $result.Stderr
    $logPath = Join-Path $OutputDir "pve-rbd-preview-$timestamp-$LogLabel-run-$Run.txt"
    [System.IO.File]::WriteAllText(
        $logPath,
        (Protect-PerformanceLog -Text $combined),
        [System.Text.UTF8Encoding]::new($false)
    )
    if ($result.ExitCode -ne 0) {
        throw "$LogLabel preview performance run $Run failed with exit code $($result.ExitCode). Log: $logPath"
    }
    $matches = [regex]::Matches(
        $combined,
        "$([regex]::Escape($MetricMarker)) (?<json>\{[^\r\n]+\})"
    )
    if ($matches.Count -ne 1) {
        throw "Run $Run emitted $($matches.Count) $MetricMarker records. Log: $logPath"
    }
    $report = $matches[0].Groups["json"].Value | ConvertFrom-Json
    if ([int]$report.schemaVersion -ne 1) {
        throw "Run $Run emitted unsupported $LogLabel metrics schema version $($report.schemaVersion)."
    }
    return [pscustomobject]@{
        run = $Run
        log = $logPath
        report = $report
    }
}

function Invoke-PreviewPerformanceRun {
    param([Parameter(Mandatory = $true)][int]$Run)

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $cargo.Source
    $startInfo.Arguments = "test -p app-services --test pve_rbd_preview_performance $testName -- --ignored --exact --nocapture --test-threads=1"
    $startInfo.WorkingDirectory = $projectRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.Environment["FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT"] = $resolvedCaseRoot

    $result = Invoke-RustGuardProcess `
        -StartInfo $startInfo `
        -TimeoutMilliseconds ($TimeoutSeconds * 1000) `
        -TimeoutContext "retained PVE RBD preview performance run $Run"
    $combined = $result.Stdout + [Environment]::NewLine + $result.Stderr
    $logPath = Join-Path $OutputDir "pve-rbd-preview-$timestamp-run-$Run.txt"
    $protectedLog = Protect-PerformanceLog -Text $combined
    [System.IO.File]::WriteAllText($logPath, $protectedLog, [System.Text.UTF8Encoding]::new($false))
    if ($result.ExitCode -ne 0) {
        throw "PVE RBD preview performance run $Run failed with exit code $($result.ExitCode). Log: $logPath"
    }

    $matches = [regex]::Matches(
        $combined,
        "PVE_RBD_PREVIEW_METRICS (?<json>\{[^\r\n]+\})"
    )
    if ($matches.Count -ne 1) {
        throw "Run $Run emitted $($matches.Count) PVE_RBD_PREVIEW_METRICS records. Log: $logPath"
    }
    $report = $matches[0].Groups["json"].Value | ConvertFrom-Json
    if ([int]$report.schemaVersion -ne 1) {
        throw "Run $Run emitted unsupported metrics schema version $($report.schemaVersion)."
    }
    return [pscustomobject]@{
        run = $Run
        log = $logPath
        report = $report
    }
}

function Assert-Maximum {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][double]$Actual,
        [Parameter(Mandatory = $true)][double]$Maximum,
        [string]$Unit = "ms"
    )

    if ($Actual -gt $Maximum) {
        throw "$Name regression: $([Math]::Round($Actual, 3))$Unit > $([Math]::Round($Maximum, 3))$Unit"
    }
}

function Assert-LifecycleReport {
    param([Parameter(Mandatory = $true)][object]$Report)

    if (-not [bool]$Report.lifecycle.mediaParity.exactMatch -or
        $Report.lifecycle.mediaParity.viewerSha256 -ne $Report.lifecycle.mediaParity.mediaSha256) {
        throw "Viewer and media preview bytes did not remain identical."
    }
    foreach ($cycleName in @("sourceInvalidation", "caseInvalidation")) {
        $cycle = $Report.lifecycle.$cycleName
        if (-not [bool]$cycle.drained -or
            -not [bool]$cycle.oldHandleRejected -or
            -not [bool]$cycle.openWhileRetiredRejected -or
            -not [bool]$cycle.fixedOracleMatch -or
            [int]$cycle.postInvalidationSessionCount -ne 0 -or
            [int]$cycle.postRebuildCloseSessionCount -ne 0) {
            throw "Preview lifecycle checkpoint '$cycleName' did not converge safely."
        }
    }
}

function Assert-FixedOracle {
    param([Parameter(Mandatory = $true)][object]$Report)

    foreach ($expectedFile in $fixedOracle.files) {
        $matches = @($Report.files | Where-Object {
            $_.label -eq $expectedFile.label -and
            $_.path -eq $expectedFile.path -and
            [int64]$_.size -eq [int64]$expectedFile.size
        })
        if ($matches.Count -ne 1) {
            throw "Fixed file oracle changed for '$($expectedFile.label)'."
        }
    }
    foreach ($expectedRange in $fixedOracle.ranges) {
        $matches = @($Report.ranges | Where-Object {
            $_.scenario -eq $expectedRange.scenario -and
            [int64]$_.offset -eq [int64]$expectedRange.offset -and
            [int]$_.requestedBytes -eq [int]$expectedRange.requestedBytes
        })
        if ($matches.Count -ne 1) {
            throw "Fixed range oracle '$($expectedRange.scenario)' was not emitted exactly once."
        }
        $actual = $matches[0]
        if ([int]$actual.actualBytes -ne [int]$expectedRange.actualBytes -or
            $actual.sha256 -ne $expectedRange.sha256) {
            throw "Fixed evidence bytes changed for '$($expectedRange.scenario)'."
        }
    }
}

Invoke-PreviewPerformanceBuild
$runResults = New-Object System.Collections.Generic.List[object]
$oracle = @{}
for ($run = 1; $run -le $Runs; $run++) {
    Write-Host "[$run/$Runs] Running retained PVE RBD preview performance regression"
    $result = Invoke-PreviewPerformanceRun -Run $run
    $report = $result.report
    Assert-FixedOracle -Report $report
    Assert-LifecycleReport -Report $report
    if ([int]$report.runtime.providerConstructions -ne 1) {
        throw "Run $run constructed $($report.runtime.providerConstructions) providers; expected exactly 1."
    }
    if ([int]$report.runtime.runtimeCount -ne 1 -or
        [int]$report.runtime.postCloseSessionCount -ne 0) {
        throw "Run $run did not preserve one runtime with deterministic session close."
    }
    if ([int64]$report.runtime.runtimeCacheCapacityBytes -gt $MaxRuntimeCacheBytes) {
        throw "Run $run runtime cache capacity exceeded the budget."
    }
    if ([int]$report.memory.rssDeltaMb -gt $MaxRssDeltaMb) {
        throw "Run $run RSS delta $($report.memory.rssDeltaMb)MB exceeded ${MaxRssDeltaMb}MB."
    }
    foreach ($range in $report.ranges) {
        $key = "$($range.scenario)|$($range.offset)|$($range.requestedBytes)"
        if ($oracle.ContainsKey($key) -and $oracle[$key] -ne $range.sha256) {
            throw "Evidence range digest changed across runs for $key."
        }
        $oracle[$key] = $range.sha256
    }
    $runResults.Add($result) | Out-Null
    Write-Host (
        "  coldRead={0:N2}ms warm64={1:N2}ms seq1MiB={2:N2}ms random64={3:N2}ms rssDelta={4}MB" -f
        (Get-Metric -Report $report -Scenario "coldSmallRead").p95Ms,
        (Get-Metric -Report $report -Scenario "warmSame64KiB").p95Ms,
        (Get-Metric -Report $report -Scenario "sequential4x1MiB").p95Ms,
        (Get-Metric -Report $report -Scenario "largeRandom64KiB").p95Ms,
        $report.memory.rssDeltaMb
    )
}

$nativeResults = New-Object System.Collections.Generic.List[object]
if ($hasNativeXfsFixture) {
    for ($run = 1; $run -le $Runs; $run++) {
        Write-Host "[$run/$Runs] Running native Linux XFS preview comparison"
        $result = Invoke-ComparisonPerformanceRun `
            -Run $run `
            -TestTarget "native_linux_preview_performance" `
            -TestName "native_linux_xfs_preview_performance" `
            -MetricMarker "NATIVE_XFS_PREVIEW_METRICS" `
            -EnvironmentName "FORENSICS_LINUX_E01_FIXTURE" `
            -EnvironmentValue $resolvedNativeXfsFixture `
            -LogLabel "native-xfs"
        if ($result.report.oracleCapture.status -ne "fixed-oracle-verified" -or
            @($result.report.skippedScenarios).Count -ne 0) {
            throw "Native XFS run $run did not verify its complete fixed oracle matrix."
        }
        $nativeResults.Add($result) | Out-Null
    }
}

$pveHostResults = New-Object System.Collections.Generic.List[object]
if ($hasPveClusterRoot) {
    for ($run = 1; $run -le $Runs; $run++) {
        Write-Host "[$run/$Runs] Running PVE host EXT4 preview comparison"
        $result = Invoke-ComparisonPerformanceRun `
            -Run $run `
            -TestTarget "pve_host_preview_performance" `
            -TestName "pve_host_ext4_native_preview_performance" `
            -MetricMarker "PVE_HOST_PREVIEW_METRICS" `
            -EnvironmentName "FORENSICS_PVE_CLUSTER_ROOT" `
            -EnvironmentValue $resolvedPveClusterRoot `
            -LogLabel "pve-host-ext4"
        if (-not [bool]$result.report.oracleVerified -or
            @($result.report.metrics | Where-Object { $_.status -ne "measured" }).Count -ne 0) {
            throw "PVE host EXT4 run $run did not verify its complete fixed oracle matrix."
        }
        $pveHostResults.Add($result) | Out-Null
    }
}

$firstReport = $runResults[0].report
foreach ($run in $runResults) {
    if ($run.report.caseId -ne $firstReport.caseId -or
        $run.report.dataSourceId -ne $firstReport.dataSourceId) {
        throw "Performance runs did not use one retained case and derived source."
    }
}

if ($nativeResults.Count -gt 0) {
    $firstNative = $nativeResults[0].report
    foreach ($run in $nativeResults) {
        if ($run.report.fixtureFingerprint.sha256 -ne $firstNative.fixtureFingerprint.sha256 -or
            $run.report.file.logicalPath -ne $firstNative.file.logicalPath -or
            [int64]$run.report.file.size -ne [int64]$firstNative.file.size -or
            ($run.report.oracleCapture.ranges | ConvertTo-Json -Compress) -ne
            ($firstNative.oracleCapture.ranges | ConvertTo-Json -Compress)) {
            throw "Native XFS comparison oracle changed across runs."
        }
    }
}
if ($pveHostResults.Count -gt 0) {
    $firstPveHost = $pveHostResults[0].report
    foreach ($run in $pveHostResults) {
        $runDigests = @($run.report.metrics | ForEach-Object { "$($_.scenario):$($_.digestSha256)" })
        $firstDigests = @($firstPveHost.metrics | ForEach-Object { "$($_.scenario):$($_.digestSha256)" })
        if ($run.report.memberFingerprint -ne $firstPveHost.memberFingerprint -or
            $run.report.logicalFilePath -ne $firstPveHost.logicalFilePath -or
            [int64]$run.report.fileSize -ne [int64]$firstPveHost.fileSize -or
            ($runDigests -join "|") -ne ($firstDigests -join "|")) {
            throw "PVE host EXT4 comparison oracle changed across runs."
        }
    }
}

$coldReadMedianMs = Get-Median @($runResults | ForEach-Object {
    [double](Get-Metric -Report $_.report -Scenario "coldSmallRead").p95Ms
})
$coldOpenReadMedianMs = Get-Median @($runResults | ForEach-Object {
    [double](Get-Metric -Report $_.report -Scenario "coldSmallOpenRead").p95Ms
})
$warmSame64KiBMedianP95Ms = Get-Median @($runResults | ForEach-Object {
    [double](Get-Metric -Report $_.report -Scenario "warmSame64KiB").p95Ms
})
$sequential16x64KiBMedianP95Ms = Get-Median @($runResults | ForEach-Object {
    [double](Get-Metric -Report $_.report -Scenario "sequential16x64KiB").p95Ms
})
$sequential4x1MiBMedianP95Ms = Get-Median @($runResults | ForEach-Object {
    [double](Get-Metric -Report $_.report -Scenario "sequential4x1MiB").p95Ms
})
$largeRandom64KiBMedianP95Ms = Get-Median @($runResults | ForEach-Object {
    [double](Get-Metric -Report $_.report -Scenario "largeRandom64KiB").p95Ms
})

Assert-Maximum "cold file read median" $coldReadMedianMs $MaxColdFileReadMedianMs
Assert-Maximum "warm same 64 KiB median p95" $warmSame64KiBMedianP95Ms $MaxWarmSame64KiBP95Ms
Assert-Maximum "sequential 16x64 KiB median p95" $sequential16x64KiBMedianP95Ms $MaxSequential16x64KiBP95Ms
Assert-Maximum "sequential 4x1 MiB median p95" $sequential4x1MiBMedianP95Ms $MaxSequential4x1MiBMedianP95Ms
Assert-Maximum "large random 64 KiB median p95" $largeRandom64KiBMedianP95Ms $MaxLargeRandom64KiBMedianP95Ms

$comparisonSummary = [ordered]@{
    nativeXfs = [ordered]@{ status = "skipped" }
    pveHostExt4 = [ordered]@{ status = "skipped" }
    rbdToNativeWarmRatio = [ordered]@{ status = "skipped" }
}
if ($nativeResults.Count -gt 0) {
    $nativeWarmMedianP95Ms = Get-Median @($nativeResults | ForEach-Object {
        [double](Get-Metric -Report $_.report -Scenario "warmSame64KiB").p95Ms
    })
    $nativeSequential4x1MiBMedianP95Ms = Get-Median @($nativeResults | ForEach-Object {
        [double](Get-Metric -Report $_.report -Scenario "sequential4x1MiB").p95Ms
    })
    Assert-Maximum "native XFS warm same 64 KiB median p95" `
        $nativeWarmMedianP95Ms $MaxNativeWarmSame64KiBP95Ms
    Assert-Maximum "native XFS sequential 4x1 MiB median p95" `
        $nativeSequential4x1MiBMedianP95Ms $MaxNativeSequential4x1MiBP95Ms

    $rawWarmRatio = if ($nativeWarmMedianP95Ms -gt 0) {
        $warmSame64KiBMedianP95Ms / $nativeWarmMedianP95Ms
    } else {
        [double]::PositiveInfinity
    }
    $gatedWarmRatio = $warmSame64KiBMedianP95Ms /
        [Math]::Max($nativeWarmMedianP95Ms, $WarmRatioNoiseFloorMs)
    Assert-Maximum "RBD/native XFS warm 64 KiB ratio with noise floor" `
        $gatedWarmRatio $MaxRbdToNativeWarmRatio "x"
    $comparisonSummary.nativeXfs = [ordered]@{
        status = "measured"
        fixtureFingerprint = $nativeResults[0].report.fixtureFingerprint.sha256
        logicalFilePath = $nativeResults[0].report.file.logicalPath
        fileSize = [int64]$nativeResults[0].report.file.size
        warmSame64KiBP95Ms = $nativeWarmMedianP95Ms
        sequential4x1MiBP95Ms = $nativeSequential4x1MiBMedianP95Ms
    }
    $comparisonSummary.rbdToNativeWarmRatio = [ordered]@{
        status = "measured"
        rawRatio = $rawWarmRatio
        gatedRatio = $gatedWarmRatio
        noiseFloorMs = $WarmRatioNoiseFloorMs
        maximum = $MaxRbdToNativeWarmRatio
    }
}
if ($pveHostResults.Count -gt 0) {
    $pveHostWarmMedianP95Ms = Get-Median @($pveHostResults | ForEach-Object {
        [double](Get-Metric -Report $_.report -Scenario "warmSame64KiB").p95Ms
    })
    $pveHostSequential4x1MiBMedianP95Ms = Get-Median @($pveHostResults | ForEach-Object {
        [double](Get-Metric -Report $_.report -Scenario "sequential4x1MiB").p95Ms
    })
    Assert-Maximum "PVE host EXT4 warm same 64 KiB median p95" `
        $pveHostWarmMedianP95Ms $MaxPveHostWarmSame64KiBP95Ms
    Assert-Maximum "PVE host EXT4 sequential 4x1 MiB median p95" `
        $pveHostSequential4x1MiBMedianP95Ms $MaxPveHostSequential4x1MiBP95Ms
    $comparisonSummary.pveHostExt4 = [ordered]@{
        status = "measured"
        memberFingerprint = $pveHostResults[0].report.memberFingerprint
        logicalFilePath = $pveHostResults[0].report.logicalFilePath
        fileSize = [int64]$pveHostResults[0].report.fileSize
        warmSame64KiBP95Ms = $pveHostWarmMedianP95Ms
        sequential4x1MiBP95Ms = $pveHostSequential4x1MiBMedianP95Ms
    }
}

$commit = "unknown"
try {
    $commitOutput = @(& git -C $projectRoot rev-parse HEAD 2>$null)
    if ($LASTEXITCODE -eq 0 -and $commitOutput.Count -gt 0) {
        $commit = $commitOutput[0].Trim()
    }
} catch {
    $commit = "unknown"
}

$summary = [ordered]@{
    schemaVersion = 1
    generatedAt = (Get-Date).ToString("yyyy-MM-ddTHH:mm:ssK")
    commit = $commit
    caseStorageFingerprint = Get-CaseStorageFingerprint -Path $resolvedCaseRoot
    caseId = $firstReport.caseId
    dataSourceId = $firstReport.dataSourceId
    cacheMode = "shared-runtime-64k-pages-request-coalescing-256k"
    runs = $Runs
    medians = [ordered]@{
        coldFileReadMs = $coldReadMedianMs
        coldRuntimeAndFileMs = $coldOpenReadMedianMs
        warmSame64KiBP95Ms = $warmSame64KiBMedianP95Ms
        sequential16x64KiBP95Ms = $sequential16x64KiBMedianP95Ms
        sequential4x1MiBP95Ms = $sequential4x1MiBMedianP95Ms
        largeRandom64KiBP95Ms = $largeRandom64KiBMedianP95Ms
    }
    thresholds = [ordered]@{
        maxColdFileReadMedianMs = $MaxColdFileReadMedianMs
        maxWarmSame64KiBP95Ms = $MaxWarmSame64KiBP95Ms
        maxSequential16x64KiBP95Ms = $MaxSequential16x64KiBP95Ms
        maxSequential4x1MiBMedianP95Ms = $MaxSequential4x1MiBMedianP95Ms
        maxLargeRandom64KiBMedianP95Ms = $MaxLargeRandom64KiBMedianP95Ms
        maxNativeWarmSame64KiBP95Ms = $MaxNativeWarmSame64KiBP95Ms
        maxNativeSequential4x1MiBP95Ms = $MaxNativeSequential4x1MiBP95Ms
        maxPveHostWarmSame64KiBP95Ms = $MaxPveHostWarmSame64KiBP95Ms
        maxPveHostSequential4x1MiBP95Ms = $MaxPveHostSequential4x1MiBP95Ms
        maxRbdToNativeWarmRatio = $MaxRbdToNativeWarmRatio
        warmRatioNoiseFloorMs = $WarmRatioNoiseFloorMs
        maxRuntimeCacheBytes = $MaxRuntimeCacheBytes
        maxRssDeltaMb = $MaxRssDeltaMb
    }
    runtime = $firstReport.runtime
    lifecycle = $firstReport.lifecycle
    comparisons = $comparisonSummary
    files = $firstReport.files
    logs = @(
        $runResults | ForEach-Object { Split-Path -Leaf $_.log }
        $nativeResults | ForEach-Object { Split-Path -Leaf $_.log }
        $pveHostResults | ForEach-Object { Split-Path -Leaf $_.log }
    )
}
$summaryPath = Join-Path $OutputDir "pve-rbd-preview-$timestamp-summary.json"
$summaryJson = $summary | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($summaryPath, $summaryJson, [System.Text.UTF8Encoding]::new($false))

Write-Host (
    "PASS: coldRead={0:N2}ms, coldTotal={1:N2}ms (reported only), warm64={2:N2}ms, seq64={3:N2}ms, seq1MiB={4:N2}ms, random64={5:N2}ms" -f
    $coldReadMedianMs,
    $coldOpenReadMedianMs,
    $warmSame64KiBMedianP95Ms,
    $sequential16x64KiBMedianP95Ms,
    $sequential4x1MiBMedianP95Ms,
    $largeRandom64KiBMedianP95Ms
)
if ($nativeResults.Count -gt 0) {
    Write-Host (
        "COMPARE: nativeWarm64={0:N2}ms, rawRatio={1:N2}x, gatedRatio={2:N2}x (noise floor {3:N2}ms)" -f
        $nativeWarmMedianP95Ms,
        $rawWarmRatio,
        $gatedWarmRatio,
        $WarmRatioNoiseFloorMs
    )
}
if ($pveHostResults.Count -gt 0) {
    Write-Host (
        "CONTROL: pveHostWarm64={0:N2}ms, pveHostSeq1MiB={1:N2}ms" -f
        $pveHostWarmMedianP95Ms,
        $pveHostSequential4x1MiBMedianP95Ms
    )
}
Write-Host "JSON: $summaryPath"
