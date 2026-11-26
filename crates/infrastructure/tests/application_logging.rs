use infrastructure::logging::{LogConfig, LogDirection, LogFilter, LogLevel, LogQuery, LogRuntime};
use std::sync::Arc;
use tracing_subscriber::prelude::*;

fn query(
    runtime: &LogRuntime,
    direction: LogDirection,
    positions: [Option<u64>; 3],
) -> infrastructure::logging::LogSnapshot {
    runtime
        .read(LogQuery {
            direction,
            positions,
            filter: LogFilter::default(),
            limit_per_level: 100,
        })
        .unwrap()
}

#[test]
fn real_tracing_events_preserve_levels_context_and_sanitized_files() {
    let directory = tempfile::tempdir().unwrap();
    let (runtime, _guard) = LogRuntime::start(LogConfig::new(directory.path().to_owned())).unwrap();
    let subscriber = tracing_subscriber::registry().with(runtime.layer());
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!(
            "import",
            case_id = "case-a",
            job_id = "job-a",
            password = "private-value"
        );
        let _entered = span.enter();
        tracing::info!(items = 5, "reading evidence");
        tracing::warn!("parser limitation");
        tracing::error!(
            error = r"D:\private\case.db",
            "password=private-value access failed"
        );
    });
    let snapshot = query(&runtime, LogDirection::Latest, [None; 3]);
    for (index, level) in LogLevel::ALL.into_iter().enumerate() {
        let record = &snapshot.pages[index].records[0];
        assert_eq!(record.level, level);
        assert_eq!(record.field("caseId", "case_id"), Some("case-a"));
        assert!(!record.fields.contains_key("password"));
    }
    runtime.flush().unwrap();
    let output = directory
        .path()
        .join(runtime.session_id())
        .join("runtime_rCURRENT.jsonl");
    let text = std::fs::read_to_string(output).unwrap();
    assert!(!text.contains("private-value"));
    assert!(!text.contains("private\\\\case.db"));
    let records: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    assert_eq!(records[2]["level"], "error");
    assert!(records[2]["message"]
        .as_str()
        .unwrap()
        .contains("<redacted>"));
}

#[test]
fn eight_threads_publish_unique_sequences_and_forward_queries_advance() {
    let directory = tempfile::tempdir().unwrap();
    let (runtime, _guard) = LogRuntime::start(LogConfig::new(directory.path().to_owned())).unwrap();
    let dispatch = Arc::new(tracing::Dispatch::new(
        tracing_subscriber::registry().with(runtime.layer()),
    ));
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let dispatch = dispatch.clone();
            scope.spawn(move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    for item in 0..100 {
                        tracing::info!(worker, item, "parallel record");
                    }
                })
            });
        }
    });
    let mut position = 0;
    let mut observed = Vec::new();
    loop {
        let page = query(&runtime, LogDirection::Forward, [Some(position); 3])
            .pages
            .remove(0);
        observed.extend(page.records.iter().map(|record| record.sequence));
        position = page.forward;
        if !page.has_more {
            break;
        }
    }
    assert_eq!(observed, (1..=800).collect::<Vec<_>>());
    runtime.flush().unwrap();
    assert_eq!(runtime.health().written_sequence, 800);
}

#[test]
fn interleaved_levels_only_report_a_gap_for_unread_evicted_records() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = LogConfig::new(directory.path().to_owned());
    config.records_per_level = [1; 3];
    let (runtime, _guard) = LogRuntime::start(config).unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        tracing::info!("previous info");
        tracing::warn!("warning");
        tracing::error!("error");
        tracing::info!("current info");
    });
    let current = query(&runtime, LogDirection::Forward, [Some(2); 3]);
    assert!(!current.pages[0].gap);
    assert_eq!(current.pages[0].records[0].sequence, 4);
    let expired = query(&runtime, LogDirection::Forward, [Some(0); 3]);
    assert!(expired.pages[0].gap);
    assert!(!expired.pages[1].gap);
}

#[test]
fn retention_truncation_filtering_and_empty_incremental_pages_are_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = LogConfig::new(directory.path().to_owned());
    config.records_per_level = [4; 3];
    let (runtime, _guard) = LogRuntime::start(config).unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        for item in 0..10 {
            tracing::info!(item, "{}", "界".repeat(10_000));
        }
    });
    let snapshot = query(&runtime, LogDirection::Forward, [Some(0); 3]);
    assert_eq!(snapshot.pages[0].records.len(), 4);
    assert_eq!(snapshot.pages[0].evicted, 6);
    assert_eq!(snapshot.pages[0].truncated, 10);
    assert!(snapshot.pages[0].gap);
    for record in &snapshot.pages[0].records {
        assert!(serde_json::to_vec(&**record).unwrap().len() <= 8192);
    }
    let filtered = runtime
        .read(LogQuery {
            direction: LogDirection::Forward,
            positions: [Some(0); 3],
            filter: LogFilter {
                text: "absent".into(),
                ..Default::default()
            },
            limit_per_level: 100,
        })
        .unwrap();
    assert!(filtered.pages[0].records.is_empty());
    assert_eq!(filtered.pages[0].forward, 10);
    assert!(runtime.record(1).is_err());
}

#[test]
fn actual_file_rotation_keeps_the_configured_archive_count() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = LogConfig::new(directory.path().to_owned());
    config.file_bytes = 8192;
    config.archives = 2;
    let (runtime, _guard) = LogRuntime::start(config).unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        for item in 0..100 {
            tracing::error!(item, "{}", "x".repeat(1900));
        }
    });
    runtime.flush().unwrap();
    let files: Vec<_> = std::fs::read_dir(directory.path().join(runtime.session_id()))
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    assert_eq!(files.len(), 3);
    for entry in files {
        assert!(entry.metadata().unwrap().len() <= 8192);
    }
    runtime.shutdown().unwrap();
}

#[test]
fn invalid_output_directory_fails_at_initialization_and_explicit_reopen_preserves_output() {
    let directory = tempfile::tempdir().unwrap();
    let invalid = directory.path().join("occupied");
    std::fs::write(&invalid, b"occupied").unwrap();
    assert!(LogRuntime::start(LogConfig::new(invalid)).is_err());
    let (runtime, _guard) =
        LogRuntime::start(LogConfig::new(directory.path().join("logs"))).unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        tracing::error!("before reopening");
    });
    runtime.flush().unwrap();
    runtime.reopen_writer().unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        tracing::error!("after reopening");
    });
    runtime.flush().unwrap();
    let path = directory
        .path()
        .join("logs")
        .join(runtime.session_id())
        .join("runtime_rCURRENT.jsonl");
    let output = std::fs::read_to_string(path).unwrap();
    assert!(output.contains("before reopening"));
    assert!(output.contains("after reopening"));
}

#[test]
#[cfg(windows)]
fn actual_rotation_failure_is_visible_and_explicit_reopen_recovers() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = tempfile::tempdir().unwrap();
    let mut config = LogConfig::new(directory.path().to_owned());
    config.file_bytes = 8192;
    let (runtime, _guard) = LogRuntime::start(config).unwrap();
    let subscriber = tracing_subscriber::registry().with(runtime.layer());
    tracing::subscriber::with_default(subscriber, || tracing::error!("before rotation"));
    runtime.flush().unwrap();
    let path = directory
        .path()
        .join(runtime.session_id())
        .join("runtime_rCURRENT.jsonl");
    let occupied = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        for item in 0..10 {
            tracing::error!(item, "{}", "r".repeat(1900));
        }
    });
    assert!(runtime.flush().is_err());
    assert!(!runtime
        .health()
        .failure
        .unwrap()
        .contains(&directory.path().to_string_lossy().into_owned()));
    assert!(runtime.health().failed_records > 0);
    drop(occupied);
    runtime.reopen_writer().unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        tracing::error!("rotation recovered");
    });
    runtime.flush().unwrap();
    assert!(runtime.health().failure.is_none());
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("rotation recovered"));
}
