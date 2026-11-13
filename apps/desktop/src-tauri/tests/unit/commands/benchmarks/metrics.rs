use persistence_source::fjall::instrumentation::{ProcessMeasurement, StorageCounters};
use std::time::Instant;

use super::{config::RunConfig, memory::MemorySampler};

#[derive(Default)]
pub(super) struct ScenarioCounters {
    pub files_scanned: Option<u64>,
    pub cursor_pages: Option<u64>,
    pub returned_rows: Option<u64>,
    pub clone_records_copied: Option<u64>,
    pub clone_bytes_copied: Option<u64>,
}

fn counters_json(storage: StorageCounters, logical: &ScenarioCounters) -> serde_json::Value {
    serde_json::json!({
        "filesScanned": logical.files_scanned,
        "cursorPages": logical.cursor_pages,
        "returnedRows": logical.returned_rows,
        "cloneRecordsCopied": logical.clone_records_copied,
        "cloneBytesCopied": logical.clone_bytes_copied,
        "pointReads": storage.point_reads,
        "scanCalls": storage.scan_calls,
        "scanEntries": storage.scan_entries,
        "recordsVisited": storage.point_reads.saturating_add(storage.scan_entries),
        "fjallDatabaseOpens": storage.database_opens,
        "sourceSessionOpens": storage.source_session_opens,
        "searchIndexOpens": storage.search_index_opens,
        "querySessionOpens": storage.query_session_opens,
        "rankedHits": storage.ranked_hits,
        "mergeComparisons": storage.merge_comparisons,
        "bytesRead": storage.bytes_read,
        "bytesWritten": storage.bytes_written,
        "writeBatches": storage.write_batches,
        "writtenActions": storage.written_actions,
        "syncAllCount": storage.sync_all_persists,
        "pathNormalizations": storage.path_normalizations,
    })
}

pub(super) fn measure(
    config: &RunConfig,
    scenario: &str,
    workload: serde_json::Value,
    mut operation: impl FnMut() -> ScenarioCounters,
) -> serde_json::Value {
    for _ in 0..config.warmups {
        operation();
    }
    let mut samples = Vec::with_capacity(config.runs as usize);
    for _ in 0..config.runs {
        let measurement = ProcessMeasurement::start().expect("exclusive benchmark measurement");
        let memory = MemorySampler::start();
        let process_start = app_services::runtime_resources::ProcessCounterSnapshot::capture();
        let started = Instant::now();
        let logical = operation();
        let elapsed = started.elapsed();
        let process_end = app_services::runtime_resources::ProcessCounterSnapshot::capture();
        let memory = memory.finish();
        let storage = measurement.snapshot();
        drop(measurement);
        samples.push(serde_json::json!({
            "elapsedNs": elapsed.as_nanos().min(u128::from(u64::MAX)) as u64,
            "elapsedMs": elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            "counters": counters_json(storage, &logical),
            "memory": memory,
            "process": process_start.usage_until(process_end, elapsed),
        }));
    }
    scenario_result(config, scenario, workload, samples)
}

fn scenario_result(
    config: &RunConfig,
    scenario: &str,
    workload: serde_json::Value,
    samples: Vec<serde_json::Value>,
) -> serde_json::Value {
    assert_eq!(samples.len(), config.runs as usize);
    let mut timings = samples
        .iter()
        .map(|sample| sample["elapsedMs"].as_u64().unwrap())
        .collect::<Vec<_>>();
    timings.sort_unstable();
    let p95 = timings[(timings.len() * 95).div_ceil(100) - 1];
    let peak = samples
        .iter()
        .filter_map(|sample| sample["memory"]["sampledPeakRssMb"].as_u64())
        .max();
    serde_json::json!({
        "scenario": scenario,
        "datasetLevel": config.level,
        "workload": workload,
        "warmupRuns": config.warmups,
        "p95Ms": p95,
        "memoryPeakMb": peak,
        "memoryMeasurement": "sampled-process-rss",
        "memorySampleIntervalMs": super::memory::SAMPLE_INTERVAL_MS,
        "processId": std::process::id(),
        "runs": samples,
    })
}
