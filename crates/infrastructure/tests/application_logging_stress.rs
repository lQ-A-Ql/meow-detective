use infrastructure::logging::{LogConfig, LogDirection, LogFilter, LogQuery, LogRuntime};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tracing_subscriber::prelude::*;

#[test]
#[ignore = "30 second logging resource acceptance; requires FORENSICS_LOG_STRESS_OUTPUT"]
fn sustained_logging_keeps_retention_and_memory_bounded() {
    let output = std::path::PathBuf::from(std::env::var_os("FORENSICS_LOG_STRESS_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let pid = sysinfo::Pid::from_u32(std::process::id());
    let mut system = sysinfo::System::new().unwrap();
    let mut memory = || {
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            true,
            sysinfo::ProcessRefreshKind::nothing()
                .with_memory()
                .without_tasks(),
        );
        system.process(pid).unwrap().memory()
    };
    let before = memory();
    assert!(before > 0, "process memory measurement must be available");
    let (runtime, _guard) = LogRuntime::start(LogConfig::new(output.join("logs"))).unwrap();
    let dispatch = Arc::new(tracing::Dispatch::new(
        tracing_subscriber::registry().with(runtime.layer()),
    ));
    let started = Instant::now();
    let mut peak = before;
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for worker in 0..8 {
            let dispatch = dispatch.clone();
            workers.push(scope.spawn(move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    for batch in 0..1500 {
                        for item in 0..25 {
                            tracing::info!(worker, batch, item, "bounded logging acceptance");
                        }
                        let deadline = started + Duration::from_millis((batch + 1) * 20);
                        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                    }
                })
            }));
        }
        while workers.iter().any(|worker| !worker.is_finished()) {
            peak = peak.max(memory());
            std::thread::sleep(Duration::from_millis(100));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    runtime.flush().unwrap();
    let snapshot = runtime
        .read(LogQuery {
            direction: LogDirection::Latest,
            positions: [None; 3],
            filter: LogFilter::default(),
            limit_per_level: 100,
        })
        .unwrap();
    assert_eq!(snapshot.pages[0].emitted, 300_000);
    assert_eq!(snapshot.pages[0].evicted, 288_000);
    assert!(peak.saturating_sub(before) < 64 * 1024 * 1024);
    assert!(snapshot.writer.failure.is_none());
    let report = serde_json::json!({
        "events": snapshot.pages[0].emitted,
        "evicted": snapshot.pages[0].evicted,
        "fileDropped": snapshot.writer.dropped,
        "failedRecords": snapshot.writer.failed_records,
        "rssBeforeBytes": before, "rssPeakBytes": peak,
        "elapsedSeconds": started.elapsed().as_secs_f64(),
    });
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
