$data = Get-Content 'D:\process\forensic\.understand-anything\tmp\ua-file-extract-results-4.json' -Raw | ConvertFrom-Json
$results = @()
foreach ($r in $data.results) {
    $funcs = @()
    foreach ($f in $r.functions) {
        $funcs += [PSCustomObject]@{
            name = $f.name
            startLine = $f.startLine
            endLine = $f.endLine
            params = $f.params
            isExported = $f.isExported
        }
    }
    $clss = @()
    foreach ($c in $r.classes) {
        $clss += [PSCustomObject]@{
            name = $c.name
            startLine = $c.startLine
            endLine = $c.endLine
            methods = $c.methods
            properties = $c.properties
            isExported = $c.isExported
        }
    }
    $exps = @()
    foreach ($e in $r.exports) {
        $exps += [PSCustomObject]@{
            name = $e.name
            line = $e.line
            isDefault = $e.isDefault
        }
    }
    $results += [PSCustomObject]@{
        path = $r.path
        language = $r.language
        fileCategory = $r.fileCategory
        totalLines = $r.totalLines
        nonEmptyLines = $r.nonEmptyLines
        functions = $funcs
        classes = $clss
        exports = $exps
        metrics = $r.metrics
        callGraphCount = @($r.callGraph).Count
        hasSections = $null -ne $r.sections
        hasDefinitions = $null -ne $r.definitions
        hasServices = $null -ne $r.services
        hasEndpoints = $null -ne $r.endpoints
        hasSteps = $null -ne $r.steps
        hasResources = $null -ne $r.resources
    }
}
$results | ConvertTo-Json -Depth 8 | Out-File -Encoding utf8 'D:\process\forensic\.understand-anything\tmp\ua-file-extract-results-4-summary.json'
Write-Output 'Done'
