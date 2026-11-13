use std::time::Instant;

use persistence_source::fjall::instrumentation;

/// Optional storage/timeline counters attached to one scenario result.
///
/// A field stays `None` unless the scenario explicitly records it, and `None`
/// is emitted as JSON `null` so partial coverage is visible to consumers.
#[derive(Default)]
pub(super) struct ScenarioCounters {
    pub(super) files_scanned: Option<u64>,
    pub(super) records_visited: Option<u64>,
    pub(super) index_opens: Option<u64>,
    pub(super) cursor_pages: Option<u64>,
    pub(super) bytes_read: Option<u64>,
    pub(super) bytes_written: Option<u64>,
    pub(super) sync_all_count: Option<u64>,
    pub(super) path_normalizations: Option<u64>,
    pub(super) lock_wait_ms: Option<u64>,
}

impl ScenarioCounters {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "filesScanned": self.files_scanned,
            "recordsVisited": self.records_visited,
            "indexOpens": self.index_opens,
            "cursorPages": self.cursor_pages,
            "bytesRead": self.bytes_read,
            "bytesWritten": self.bytes_written,
            "syncAllCount": self.sync_all_count,
            "pathNormalizations": self.path_normalizations,
            "lockWaitMs": self.lock_wait_ms,
        })
    }
}

pub(super) fn measure_runs(runs: u32, mut operation: impl FnMut()) -> Vec<u64> {
    let mut elapsed = Vec::with_capacity(runs as usize);
    for _ in 0..runs {
        let started = Instant::now();
        operation();
        elapsed.push(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    }
    elapsed
}

/// Measures `runs` executions while collecting fjall storage instrumentation.
///
/// Counters are reset before every run and only the last measured run is
/// reported, so one scenario result always describes an identical amount of
/// work (one operation) rather than an accumulation over the whole batch.
pub(super) fn measure_runs_with_counters(
    runs: u32,
    mut operation: impl FnMut(),
) -> (Vec<u64>, ScenarioCounters) {
    let mut elapsed = Vec::with_capacity(runs as usize);
    if runs == 0 {
        return (elapsed, ScenarioCounters::default());
    }
    instrumentation::enable();
    let mut storage = None;
    for _ in 0..runs {
        instrumentation::reset();
        let started = Instant::now();
        operation();
        elapsed.push(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        storage = Some(instrumentation::snapshot());
    }
    instrumentation::disable();
    let storage = storage.expect("at least one measured run records storage counters");
    let counters = ScenarioCounters {
        records_visited: Some(storage.scan_entries.saturating_add(storage.point_reads)),
        index_opens: Some(storage.database_opens),
        bytes_read: Some(storage.bytes_read),
        bytes_written: Some(storage.bytes_written),
        sync_all_count: Some(storage.sync_all_persists),
        ..ScenarioCounters::default()
    };
    (elapsed, counters)
}

pub(super) fn p95_ms(mut timings: Vec<u64>) -> u64 {
    if timings.is_empty() {
        return 0;
    }
    timings.sort_unstable();
    let index = ((timings.len() as f64) * 0.95).ceil() as usize;
    timings[index.saturating_sub(1)]
}

pub(super) fn peak_memory_mb() -> u64 {
    app_services::import_analysis::current_rss_mb()
}

pub(super) fn scenario_result(
    scenario: &str,
    timings: Vec<u64>,
    warmup_runs: u32,
    counters: ScenarioCounters,
) -> serde_json::Value {
    serde_json::json!({
        "scenario": scenario,
        "datasetLevel": std::env::var("FORENSICS_BENCH_FIXTURE_LEVEL")
            .unwrap_or_else(|_| "small".to_owned()),
        "warmupRuns": warmup_runs,
        "p95Ms": p95_ms(timings.clone()),
        "memoryPeakMb": peak_memory_mb(),
        "processId": std::process::id(),
        "counters": counters.json(),
        "runs": timings
            .iter()
            .map(|elapsed| serde_json::json!({"elapsedMs": elapsed}))
            .collect::<Vec<_>>(),
    })
}
