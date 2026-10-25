use super::*;
use domain::FileEntryId;
use std::sync::{atomic::Ordering, Arc};

#[test]
fn cancellation_after_written_batches_preserves_previous_outputs_and_checkpoint() {
    let connection = Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&connection).unwrap();
    connection.execute_batch("INSERT INTO data_sources(id,case_id,name,kind,source_path,imported_at)
        VALUES('source-browser','case-browser','Browser','directory','evidence','2026-10-26T00:00:00Z');
        INSERT INTO artifacts(id,case_id,data_source_id,artifact_type,source_object_id,extractor_id,title,summary,attrs)
        VALUES('previous','case-browser','source-browser','BrowserHistory','history','browser.history','Previous','Previous','{}');").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    connection
        .create_scalar_function(
            "cancel_browser",
            0,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8,
            move |_| {
                signal.store(true, Ordering::Release);
                Ok(0)
            },
        )
        .unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER cancel_browser_write AFTER INSERT ON artifacts
        WHEN json_extract(NEW.attrs,'$.visitId')='300' BEGIN SELECT cancel_browser(); END;",
        )
        .unwrap();
    let input = Connection::open_in_memory().unwrap();
    input
        .execute_batch(
            "CREATE TABLE urls(id INTEGER PRIMARY KEY,url TEXT,title TEXT,visit_count INTEGER);
        CREATE TABLE visits(id INTEGER PRIMARY KEY,url INTEGER,visit_time INTEGER);
        INSERT INTO urls VALUES(1,'https://example.test/','Page',1107);",
        )
        .unwrap();
    for id in 1..=1107 {
        input
            .execute("INSERT INTO visits VALUES(?1,1,13300000000000001)", [id])
            .unwrap();
    }
    let bytes = input
        .serialize(rusqlite::DatabaseName::Main)
        .unwrap()
        .to_vec();
    let candidate = EvidenceCandidate {
        file_id: FileEntryId("history".into()),
        data_source_id: "source-browser".into(),
        partition_index: None,
        path: "Users/A/AppData/Local/Google/Chrome/User Data/Default/History".into(),
        size: bytes.len() as u64,
        encrypted: false,
        content_identity: "input-one".into(),
        companions: Vec::new(),
        evidence_kind: "browser".into(),
        parser: "browser.history".into(),
        category: "BrowserHistory".into(),
        modified_at: None,
    };
    let preload = BrowserPreloadContext::empty();
    let context = PersistenceContext {
        connection: &connection,
        case_id: "case-browser",
        capability: crate::analysis_service::capability::find_capability("BrowserHistory").unwrap(),
        preload: &preload,
        cancel: cancel.as_ref(),
    };
    assert!(matches!(
        persist(context, &candidate, &bytes, &[]),
        Err(AnalysisServiceError::Cancelled)
    ));
    assert!(cancel.load(Ordering::Acquire));
    let ids: Vec<String> = connection
        .prepare("SELECT id FROM artifacts")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(ids, vec!["previous"]);
    let checkpoints: i64 = connection
        .query_row(
            "SELECT count(*) FROM source_meta WHERE key LIKE 'analysis_candidate_scan:%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(checkpoints, 0);
}
