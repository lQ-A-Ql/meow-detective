use std::time::Instant;

pub(super) fn measure_runs(runs: u32, mut operation: impl FnMut()) -> Vec<u64> {
    let mut elapsed = Vec::with_capacity(runs as usize);
    for _ in 0..runs {
        let started = Instant::now();
        operation();
        elapsed.push(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    }
    elapsed
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

pub(super) fn scenario_result(scenario: &str, timings: Vec<u64>) -> serde_json::Value {
    serde_json::json!({
        "scenario": scenario,
        "datasetLevel": "small",
        "p95Ms": p95_ms(timings.clone()),
        "memoryPeakMb": peak_memory_mb(),
        "runs": timings
            .iter()
            .map(|elapsed| serde_json::json!({"elapsedMs": elapsed}))
            .collect::<Vec<_>>(),
    })
}
