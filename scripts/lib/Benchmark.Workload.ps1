function Assert-BenchmarkWorkload {
  param($Scenario, $Catalog, [string]$Level)
  if ($null -eq $Catalog) { throw 'Benchmark catalog is required for workload validation' }
  $scale = $Catalog.levels.$Level
  $work = $Scenario.workload
  $expected = switch ($Scenario.scenario) {
    'reprocess_generation_clone' {
      if ($work.totalEntries -lt $work.payloadRecords -or $work.totalLogicalBytes -lt $work.payloadRecords * $work.payloadValueBytes -or
          $work.populatedKeyspaces -lt 1 -or $work.totalKeyspaces -lt $work.populatedKeyspaces) {
        throw 'Clone source inventory is inconsistent'
      }
      @{ payloadRecords = $scale.cloneRecords; payloadValueBytes = 128 }
    }
    'timeline_native_cursor_p50' { @{ records = $scale.timelineRecords; pageSize = 50; sources = 1 } }
    'timeline_native_cursor_p256' { @{ records = $scale.timelineRecords; pageSize = 256; sources = 1 } }
    'timeline_native_facets_full' { @{ records = $scale.facetRecords; matchingRecords = $scale.facetRecords; buckets = 64; sources = 1; requests = 1 } }
    'timeline_native_facets_narrow' {
      $matching = [Math]::Floor($scale.facetRecords * 55 / 100) - [Math]::Floor($scale.facetRecords * 45 / 100) + 1
      @{ records = $scale.facetRecords; matchingRecords = $matching; buckets = 64; sources = 1; requests = 1 }
    }
    'artifact_native_facets' { @{ records = $scale.artifactRecords; sources = 1; types = 1; requests = 2 } }
    'case_search_cross_source' { @{ sources = $scale.searchSources; filesPerSource = $scale.searchFilesPerSource; pageSize = 50; pages = 2; scanEnd = 50 } }
    'case_search_cross_source_offset' { @{ sources = $scale.searchSources; filesPerSource = $scale.searchFilesPerSource; pageSize = 50; pages = 1; scanEnd = 1000 } }
    'correlation_paths' { @{ files = $scale.correlationFiles; artifacts = 250; sources = 1 } }
    'file_header_reads' { @{ files = $scale.headerFiles; sources = 1; headerBytes = 64 } }
    default {
      if ($Scenario.scenario -notin $Catalog.smoke) { throw "Unknown workload: $($Scenario.scenario)" }
      @{ files = $scale.coreFiles; sources = 1; requests = 1 }
    }
  }
  foreach ($key in $expected.Keys) {
    if ($key -notin $work.PSObject.Properties.Name -or $null -eq $work.$key -or
        [long]$work.$key -ne [long]$expected[$key]) {
      throw "Workload dimension $key differs from catalog for $($Scenario.scenario)/$Level"
    }
  }
}
