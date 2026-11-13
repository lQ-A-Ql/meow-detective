use super::*;
use chrono::Utc;
use domain::{Artifact, ArtifactId, FileEntryId, TimelineEvent, TimelineEventId};
use persistence_source::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

const CASE_ID: &str = "case-browser";
const DATA_SOURCE_ID: &str = "source-browser";

#[test]
fn cancellation_drops_the_unpublished_generation_without_changing_published_outputs() {
    let fixture = native_source();
    let cancel = AtomicBool::new(true);
    let (bytes, candidate) = browser_candidate();
    let preload = BrowserPreloadContext::empty();
    let replacements = [
        crate::analysis_service::extraction::state::AnalysisOutputReplacement {
            source_object_id: candidate.file_id.0.clone(),
            producer: crate::analysis_service::extraction::state::AnalysisOutputProducer::Prefix(
                "browser.",
            ),
        },
    ];
    let output = crate::analysis_service::extraction::native_output::begin_streaming(
        &fixture.connection,
        DATA_SOURCE_ID,
        &replacements,
        &AtomicBool::new(false),
        |_| {},
    )
    .expect("stage browser generation");

    assert!(matches!(
        persist_worker(&output, &candidate, &bytes, &[], &preload, &cancel),
        Err(AnalysisServiceError::Cancelled)
    ));
    drop(output);
    assert_previous_outputs_unchanged(&fixture.connection);
    assert_eq!(checkpoint_count(&fixture.connection), 0);
    assert_no_build_generation(&fixture.source_dir);
}

struct NativeSource {
    _root: tempfile::TempDir,
    source_dir: std::path::PathBuf,
    connection: Connection,
}

fn native_source() -> NativeSource {
    let root = tempfile::tempdir().expect("create native source fixture");
    let source_dir = root.path().join("case/sources").join(DATA_SOURCE_ID);
    std::fs::create_dir_all(&source_dir).expect("create source directory");
    let database_path = source_dir.join("source.db");
    let connection =
        persistence_sqlite::open_or_create_source(&database_path).expect("open source database");
    persistence_source::detail::configure_source_identity(&connection, DATA_SOURCE_ID)
        .expect("configure source identity");
    persistence_source::detail::configure_source_fjall(&connection, &database_path)
        .expect("configure source storage");
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, ?2, 'Browser', 'directory', 'evidence',
                     '2026-10-26T00:00:00Z')",
            rusqlite::params![DATA_SOURCE_ID, CASE_ID],
        )
        .expect("register source");
    let store = persistence_source::fjall::open_shared_source_store(
        source_dir.join("source.fjall"),
        persistence_source::fjall::DatabaseConfig::default(),
    )
    .expect("open source store");
    persistence_source::fjall::ArtifactRecordStore::from_source_store((*store).clone())
        .upsert(
            &persistence_source::detail::artifact_record(&previous_artifact(), DATA_SOURCE_ID),
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .expect("seed previous artifact");
    persistence_source::fjall::TimelineRecordStore::from_source_store((*store).clone())
        .upsert(
            &persistence_source::detail::timeline_record(&previous_event(), DATA_SOURCE_ID),
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .expect("seed previous timeline event");
    persistence_source::fjall::seal_source_generation_with_control(
        &store,
        DATA_SOURCE_ID,
        "fixture",
        &"a".repeat(64),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("seal native source fixture");
    drop(store);
    NativeSource {
        _root: root,
        source_dir,
        connection,
    }
}

fn browser_candidate() -> (Vec<u8>, EvidenceCandidate) {
    let mut input = Connection::open_in_memory().expect("open browser input");
    input
        .execute_batch(
            "CREATE TABLE urls(id INTEGER PRIMARY KEY,url TEXT,title TEXT,visit_count INTEGER);
             CREATE TABLE visits(id INTEGER PRIMARY KEY,url INTEGER,visit_time INTEGER);
             INSERT INTO urls VALUES(1,'https://example.test/','Page',1107);",
        )
        .expect("create browser input");
    let mut statement = input
        .prepare("INSERT INTO visits VALUES(?1,1,13300000000000001)")
        .expect("prepare browser visits");
    for id in 1..=1107 {
        statement.execute([id]).expect("insert browser visit");
    }
    drop(statement);
    let bytes = input
        .serialize("main")
        .expect("serialize browser input")
        .to_vec();
    let candidate = EvidenceCandidate {
        file_id: FileEntryId("history".into()),
        data_source_id: DATA_SOURCE_ID.into(),
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
    (bytes, candidate)
}

fn previous_artifact() -> Artifact {
    Artifact {
        id: ArtifactId("previous".into()),
        family: "BrowserHistory".into(),
        source_object_id: Some(FileEntryId("history".into())),
        title: "Previous".into(),
        summary: "Previous".into(),
        extractor_id: Some("browser.history".into()),
        extractor_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: None,
        created_at: Utc::now(),
        attrs: BTreeMap::new(),
    }
}

fn previous_event() -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId("previous-event".into()),
        source_object_id: "history".into(),
        event_type: "WEB_VISIT".into(),
        timestamp: Utc::now(),
        title: "Previous".into(),
        description: "Previous".into(),
        parser_id: Some("browser.history".into()),
        parser_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

fn assert_previous_outputs_unchanged(connection: &Connection) {
    let artifacts = ArtifactRepo::new(connection)
        .list_analysis_outputs("history", "browser.history")
        .expect("list browser artifacts");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].id.0, "previous");
    let events = TimelineRepo::new(connection)
        .list_analysis_outputs("history", "browser.history")
        .expect("list browser timeline events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id.0, "previous-event");
}

fn checkpoint_count(connection: &Connection) -> i64 {
    connection
        .query_row(
            "SELECT COUNT(*) FROM source_meta
             WHERE key GLOB 'analysis_candidate_scan:*'",
            [],
            |row| row.get(0),
        )
        .expect("count checkpoints")
}

fn assert_no_build_generation(source_dir: &std::path::Path) {
    let leaked = std::fs::read_dir(source_dir)
        .expect("read source directory")
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("source.fjall.build.")
        });
    assert!(!leaked, "failed streaming output left a build generation");
}
