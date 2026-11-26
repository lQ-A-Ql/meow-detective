use app_services::application_log_service::ApplicationLogService;
use infrastructure::logging::{LogConfig, LogRuntime};
use tracing_subscriber::prelude::*;
use transport::dto::*;

fn request(
    direction: ApplicationLogDirectionDto,
    cursors: Vec<String>,
) -> ApplicationLogRequestDto {
    ApplicationLogRequestDto {
        direction,
        cursors,
        filter: ApplicationLogFilterDto::default(),
        limit_per_level: 2,
    }
}

#[test]
fn real_service_pages_serialize_and_reject_foreign_cursor_contexts() {
    let directory = tempfile::tempdir().unwrap();
    let (runtime, _guard) = LogRuntime::start(LogConfig::new(directory.path().to_owned())).unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        for item in 0..5 {
            tracing::info!(item, "service log");
        }
        tracing::warn!("warning");
        tracing::error!("error");
    });
    let service = ApplicationLogService::new(runtime.clone());
    let first = service
        .read(request(ApplicationLogDirectionDto::Latest, vec![]))
        .unwrap();
    assert_eq!(
        first.pages[0]
            .records
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [4, 5]
    );
    let json = serde_json::to_value(&first).unwrap();
    let decoded: ApplicationLogSnapshotDto = serde_json::from_value(json).unwrap();
    if let Some(path) = std::env::var_os("FORENSICS_LOG_UI_FIXTURE") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_vec_pretty(&decoded).unwrap()).unwrap();
        let mut filtered = request(ApplicationLogDirectionDto::Latest, vec![]);
        filtered.filter.text = "absent".into();
        let empty = service.read(filtered).unwrap();
        std::fs::write(
            path.with_file_name("empty.json"),
            serde_json::to_vec_pretty(&empty).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(decoded.pages[1].level, ApplicationLogLevelDto::Warning);
    let backward = first
        .pages
        .iter()
        .map(|page| page.backward_cursor.clone())
        .collect();
    let previous = service
        .read(request(ApplicationLogDirectionDto::Backward, backward))
        .unwrap();
    assert_eq!(
        previous.pages[0]
            .records
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        tracing::info!("new record");
    });
    let forward: Vec<_> = first
        .pages
        .iter()
        .map(|page| page.forward_cursor.clone())
        .collect();
    let next = service
        .read(request(
            ApplicationLogDirectionDto::Forward,
            forward.clone(),
        ))
        .unwrap();
    assert_eq!(next.pages[0].records.len(), 1);
    assert_eq!(next.pages[0].records[0].message, "new record");
    let mut changed = request(ApplicationLogDirectionDto::Forward, forward);
    changed.filter.job_id = "foreign-job".into();
    assert!(service.read(changed).is_err());
    assert!(service.record("foreign-session", 1).is_err());
    assert_eq!(
        service.record(runtime.session_id(), 1).unwrap().message,
        "service log"
    );
}

#[test]
fn escaped_filters_keep_response_and_cursor_sizes_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let (runtime, _guard) = LogRuntime::start(LogConfig::new(directory.path().to_owned())).unwrap();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(runtime.layer()), || {
        for item in 0..100 {
            tracing::info!(item, context = %"\n".repeat(500), "{}", "message ".repeat(300));
            tracing::warn!(item, context = %"\n".repeat(500), "{}", "message ".repeat(300));
            tracing::error!(item, context = %"\n".repeat(500), "{}", "message ".repeat(300));
        }
    });
    let service = ApplicationLogService::new(runtime);
    let mut unfiltered = request(ApplicationLogDirectionDto::Latest, vec![]);
    unfiltered.limit_per_level = 100;
    let populated = service.read(unfiltered).unwrap();
    assert!(populated.pages.iter().all(|page| !page.records.is_empty()));
    assert!(serde_json::to_vec(&populated).unwrap().len() < 128 * 1024);
    let filter = ApplicationLogFilterDto {
        text: "\n".repeat(256),
        target: "\n".repeat(256),
        case_id: "\n".repeat(256),
        data_source_id: "\n".repeat(256),
        job_id: "\n".repeat(256),
    };
    let mut initial = request(ApplicationLogDirectionDto::Latest, vec![]);
    initial.filter = filter.clone();
    let snapshot = service.read(initial).unwrap();
    assert!(serde_json::to_vec(&snapshot).unwrap().len() < 128 * 1024);
    assert!(snapshot
        .pages
        .iter()
        .all(|page| { page.forward_cursor.len() < 512 && page.backward_cursor.len() < 512 }));
    let mut forward = request(
        ApplicationLogDirectionDto::Forward,
        snapshot
            .pages
            .into_iter()
            .map(|page| page.forward_cursor)
            .collect(),
    );
    forward.filter = filter;
    assert!(service.read(forward).is_ok());
}
