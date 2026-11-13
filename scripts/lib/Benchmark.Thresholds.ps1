function Test-BenchmarkStructure {
  param($Scenario)
  $first = $true
  foreach ($sample in $Scenario.runs) {
    $work = $Scenario.workload
    $counters = $sample.counters
    switch ($Scenario.scenario) {
      'reprocess_generation_clone' {
        if ($counters.syncAllCount -ne 1) { throw 'Generation clone must use exactly one SyncAll' }
        if ($counters.cloneRecordsCopied -ne $work.totalEntries -or $counters.cloneBytesCopied -ne $work.totalLogicalBytes) {
          throw 'Clone completion does not match the measured source inventory'
        }
        if ($counters.returnedRows -ne $work.totalEntries -or $counters.bytesWritten -lt $work.totalLogicalBytes) {
          throw 'Clone did not copy the complete source inventory'
        }
      }
      { $_ -in @('timeline_native_cursor_p50', 'timeline_native_cursor_p256') } {
        if ($counters.pointReads -le 0) { throw 'Timeline cursor storage instrumentation is absent' }
        if ($counters.returnedRows -ne $work.records) { throw 'Timeline cursor omitted records' }
        if ($counters.recordsVisited -gt 8 * $work.records + 1024) { throw 'Timeline cursor revisited the consumed prefix' }
      }
      { $_ -in @('timeline_native_facets_narrow', 'timeline_native_facets_full') } {
        if ($counters.pointReads -le 0) { throw 'Timeline facet storage instrumentation is absent' }
        if ($counters.returnedRows -ne $work.matchingRecords) { throw 'Timeline facets returned an incorrect total' }
        if ($counters.pointReads -gt $work.matchingRecords + 3000) { throw 'Timeline facets read outside the bounded window' }
        if ($counters.sourceSessionOpens -gt $work.sources * $work.requests) {
          throw 'Timeline facets opened more than one source session per request'
        }
      }
      'artifact_native_facets' {
        if ($counters.pointReads -le 0) { throw 'Artifact facet storage instrumentation is absent' }
        if ($counters.scanEntries -gt 64 * $work.sources -or $counters.pointReads -gt 64 * $work.sources) {
          throw 'Artifact facets scanned record payloads'
        }
        if ($counters.sourceSessionOpens -gt $work.sources * $work.requests) {
          throw 'Artifact facets reopened source sessions within a request'
        }
      }
      'case_search_cross_source' {
        if ($first -and $Scenario.warmupRuns -eq 0 -and $counters.searchIndexOpens -le 0) {
          throw 'Search index-open instrumentation is absent'
        }
        if ($counters.searchIndexOpens -gt $work.sources) { throw 'Search reopened source indexes on continuation' }
        if ($counters.returnedRows -ne 100) { throw 'Search continuation returned an incorrect row count' }
        if ($counters.rankedHits -gt $work.sources * $work.pageSize * $work.pages) {
          throw 'Cursor search ranked more than the per-page source budget'
        }
        $comparisonBudget = 10 * ($work.sources + $counters.returnedRows) * [Math]::Ceiling([Math]::Log($work.sources + 1, 2))
        if ($counters.mergeComparisons -le 0 -or $counters.mergeComparisons -gt $comparisonBudget) {
          throw 'Search merge comparison count is outside the heap budget'
        }
      }
      'case_search_cross_source_offset' {
        if ($first -and $Scenario.warmupRuns -eq 0 -and $counters.searchIndexOpens -le 0) {
          throw 'Search index-open instrumentation is absent'
        }
        if ($counters.searchIndexOpens -gt $work.sources) { throw 'Offset search reopened source indexes' }
        if ($counters.returnedRows -ne 50) { throw 'Offset search returned an incorrect row count' }
        if ($counters.rankedHits -gt $work.sources * $work.scanEnd) {
          throw 'Offset search exceeded its bounded ranking window'
        }
      }
      'correlation_paths' {
        if ($first -and $Scenario.warmupRuns -eq 0 -and $counters.pathNormalizations -le 0) {
          throw 'Correlation path instrumentation is absent'
        }
        if ($counters.pathNormalizations -gt $work.files + 8 * $work.artifacts) {
          throw 'Correlation repeated file-path normalization'
        }
        if ($counters.returnedRows -lt 1) { throw 'Correlation produced no matching leads' }
      }
      'file_header_reads' {
        if ($counters.filesScanned -ne $work.files) { throw 'Header benchmark omitted files' }
      }
      { $_ -in @('search_query', 'file_tree_expand', 'file_paginate', 'timeline_filter', 'artifact_list', 'report_export') } {
        if ($null -eq $counters.returnedRows -or $counters.returnedRows -le 0) {
          throw 'Core benchmark produced an empty result'
        }
      }
    }
    $first = $false
  }
}
