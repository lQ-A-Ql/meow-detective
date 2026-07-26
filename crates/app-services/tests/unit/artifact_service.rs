use super::*;
use chrono::Utc;
use domain::{
    Artifact, ArtifactId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
    EntryType, FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use std::{
    collections::BTreeMap,
    io::Read,
    sync::atomic::{AtomicUsize, Ordering},
    sync::Arc,
    sync::Barrier,
    time::Duration,
};

const ARTIFACTS_SCHEMA: &str =
    include_str!("../../../persistence-sqlite/src/migrations/scripts/0004_artifacts.sql");

fn in_memory_db_with_artifacts() -> rusqlite::Connection {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(ARTIFACTS_SCHEMA).unwrap();
    conn.execute_batch(
        "ALTER TABLE artifacts ADD COLUMN extractor_id TEXT;
         ALTER TABLE artifacts ADD COLUMN extractor_version TEXT;
         ALTER TABLE artifacts ADD COLUMN confidence REAL;
         ALTER TABLE artifacts ADD COLUMN source_attribution TEXT;",
    )
    .unwrap();
    conn
}

fn in_memory_case_db() -> rusqlite::Connection {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case = domain::CaseMeta {
        id: domain::CaseId("case-1".to_string()),
        name: "case".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
        .create(&case)
        .unwrap();
    let source = domain::DataSource {
        id: DataSourceId("ds-1".to_string()),
        name: "logical".to_string(),
        kind: domain::DataSourceKind::LogicalDirectory,
        source_path: std::path::PathBuf::from("C:/fixture"),
        imported_at: Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&conn)
        .insert(&domain::CaseId("case-1".to_string()), &source)
        .unwrap();
    conn.execute_batch("UPDATE data_sources SET import_state='ready',platform='linux'")
        .unwrap();
    conn
}

fn make_artifact(family: &str, title: &str) -> Artifact {
    Artifact {
        id: ArtifactId(uuid::Uuid::new_v4().to_string()),
        family: family.to_string(),
        title: title.to_string(),
        summary: format!("summary for {title}"),
        source_object_id: Some(FileEntryId("src-1".to_string())),
        extractor_id: None,
        extractor_version: None,
        confidence: None,
        source_attribution: None,
        created_at: Utc::now(),
        attrs: BTreeMap::new(),
    }
}

fn make_artifact_at(id: &str, created_at: &str) -> Artifact {
    let mut artifact = make_artifact("EventLog", id);
    artifact.id = ArtifactId(id.to_string());
    artifact.created_at = chrono::DateTime::parse_from_rfc3339(created_at)
        .unwrap()
        .with_timezone(&Utc);
    artifact
}

fn register_ready_source(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &str,
) -> rusqlite::Connection {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: case_root.join(format!("{source_id}.E01")),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(case_conn)
        .insert_with_storage(&domain::CaseId("case-1".to_string()), &source, &storage)
        .unwrap();
    let source_conn = crate::source_db::open_source_db(case_root, &source.id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&domain::CaseId("case-1".to_string()), &source)
        .unwrap();
    source_conn
}

fn make_file(id: &str, path: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: DataSourceId("ds-1".to_string()),
        path: path.to_string(),
        name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
        entry_type: EntryType::File,
        size: Some(1),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn insert_files(conn: &rusqlite::Connection, files: &[FileEntry]) {
    persistence_sqlite::repositories::file_repo::FileRepo::new(conn)
        .insert_batch(files)
        .unwrap();
}

#[test]
fn registry_and_unmatched_extraction_are_stable() {
    let registry = create_registry();
    assert!(registry.all().len() >= 6);
    let mut sink = artifacts_core::VecSink::new();
    let stats = run_extractors_on_file(
        &registry,
        &FileEntryId("f1".to_string()),
        "/some/random/file.xyz",
        Box::new(std::io::Cursor::new(b"hello world".to_vec())),
        &mut sink,
    )
    .unwrap();
    assert_eq!(stats, ArtifactExtractionStats::default());
    assert!(sink.artifacts.is_empty());
}

#[test]
fn artifact_persistence_and_queries_preserve_family_data() {
    let conn = in_memory_db_with_artifacts();
    let artifacts = vec![
        make_artifact("Prefetch", "pf-1"),
        make_artifact("Prefetch", "pf-2"),
        make_artifact("LNK", "lnk-1"),
    ];
    store_artifacts(&conn, &artifacts, "case-1", "ds-1").unwrap();

    assert_eq!(get_artifact_rows_from_db(&conn, None).unwrap().len(), 3);
    assert_eq!(
        get_artifact_rows_from_db(&conn, Some("LNK")).unwrap().len(),
        1
    );
    assert_eq!(get_artifact_families_from_db(&conn).unwrap().len(), 2);
    let counts = get_artifact_family_counts(&conn).unwrap();
    assert_eq!(
        counts
            .iter()
            .find(|count| count.family == "Prefetch")
            .unwrap()
            .count,
        2
    );
}

#[test]
fn empty_artifact_batch_is_a_noop() {
    let conn = in_memory_db_with_artifacts();
    store_artifacts(&conn, &[], "case-1", "ds-1").unwrap();
    assert!(get_artifact_rows_from_db(&conn, None).unwrap().is_empty());
}

#[test]
fn case_queries_route_ready_sources_and_scope_ids() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db();
    let source_id = DataSourceId("ds-1".to_string());
    let source_conn = crate::source_db::open_source_db(temp.path(), &source_id).unwrap();
    store_artifacts(
        &source_conn,
        &[make_artifact("LinuxBashCommand", "bash-history")],
        "case-1",
        "ds-1",
    )
    .unwrap();

    let rows = get_artifact_rows_for_case(
        &case_conn,
        temp.path(),
        &domain::CaseId("case-1".to_string()),
        None,
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].id.starts_with("ds:ds-1:"));
    assert_eq!(rows[0].source_object_id.as_deref(), Some("ds:ds-1:src-1"));

    let counts = get_artifact_family_counts_for_case(
        &case_conn,
        temp.path(),
        &domain::CaseId("case-1".to_string()),
    )
    .unwrap();
    assert_eq!(counts[0].family, "LinuxBashCommand");
    assert_eq!(counts[0].count, 1);
}

#[test]
fn case_artifact_pages_merge_sources_with_stable_order_and_offsets() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db();
    let first_source = DataSourceId("ds-1".to_string());
    let first_conn = crate::source_db::open_source_db(temp.path(), &first_source).unwrap();
    let second_conn = register_ready_source(&case_conn, temp.path(), "ds-2");
    store_artifacts(
        &first_conn,
        &[
            make_artifact_at("same-time", "2026-06-04T00:00:00Z"),
            make_artifact_at("first-old", "2026-06-01T00:00:00Z"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    store_artifacts(
        &second_conn,
        &[
            make_artifact_at("same-time", "2026-06-04T00:00:00Z"),
            make_artifact_at("second-middle", "2026-06-03T00:00:00Z"),
            make_artifact_at("second-old", "2026-06-02T00:00:00Z"),
        ],
        "case-1",
        "ds-2",
    )
    .unwrap();
    drop(first_conn);
    drop(second_conn);

    let page = get_artifact_rows_page_for_case(
        &case_conn,
        temp.path(),
        &domain::CaseId("case-1".to_string()),
        None,
        1,
        3,
    )
    .unwrap();

    assert_eq!(page.total, 5);
    assert_eq!(
        page.items
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ds:ds-2:same-time",
            "ds:ds-2:second-middle",
            "ds:ds-2:second-old",
        ]
    );

    let beyond = get_artifact_rows_page_for_case(
        &case_conn,
        temp.path(),
        &domain::CaseId("case-1".to_string()),
        None,
        5,
        10,
    )
    .unwrap();
    assert_eq!(beyond.total, 5);
    assert!(beyond.items.is_empty());
}

#[test]
fn legacy_case_artifact_rows_keep_the_existing_thousand_row_cap() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db();
    let source =
        crate::source_db::open_source_db(temp.path(), &DataSourceId("ds-1".to_string())).unwrap();
    let artifacts = (0..1_001)
        .map(|index| {
            make_artifact_at(
                &format!("artifact-{index:04}"),
                &format!("2026-06-01T00:{:02}:{:02}Z", (index / 60) % 60, index % 60),
            )
        })
        .collect::<Vec<_>>();
    store_artifacts(&source, &artifacts, "case-1", "ds-1").unwrap();
    drop(source);

    let rows = get_artifact_rows_for_case(
        &case_conn,
        temp.path(),
        &domain::CaseId("case-1".to_string()),
        None,
    )
    .unwrap();

    assert_eq!(rows.len(), 1_000);
}

#[test]
fn case_artifact_cursor_freezes_snapshot_and_tracks_only_consumed_keys() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db();
    let first_source = DataSourceId("ds-1".to_string());
    let first_conn = crate::source_db::open_source_db(temp.path(), &first_source).unwrap();
    let second_conn = register_ready_source(&case_conn, temp.path(), "ds-2");
    store_artifacts(
        &first_conn,
        &[
            make_artifact_at("same-a", "2026-06-04T00:00:00Z"),
            make_artifact_at("same-b", "2026-06-04T00:00:00Z"),
            make_artifact_at("old", "2026-06-01T00:00:00Z"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    store_artifacts(
        &second_conn,
        &[make_artifact_at("same-a", "2026-06-04T00:00:00Z")],
        "case-1",
        "ds-2",
    )
    .unwrap();
    drop(first_conn);
    drop(second_conn);

    let case_id = domain::CaseId("case-1".to_string());
    let first = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        2,
        None,
    )
    .unwrap();
    assert_eq!(first.total, 4);
    assert_eq!(
        first
            .items
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["ds:ds-1:same-a", "ds:ds-1:same-b"]
    );
    let cursor = first
        .next_cursor
        .expect("cursor for remaining snapshot rows");

    let second_conn =
        crate::source_db::open_source_db(temp.path(), &DataSourceId("ds-2".to_string())).unwrap();
    store_artifacts(
        &second_conn,
        &[make_artifact_at("inserted-later", "2026-06-05T00:00:00Z")],
        "case-1",
        "ds-2",
    )
    .unwrap();
    drop(second_conn);

    let second = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        10,
        Some(&cursor),
    )
    .unwrap();
    assert_eq!(second.total, 4);
    assert_eq!(
        second
            .items
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["ds:ds-2:same-a", "ds:ds-1:old"]
    );
    assert!(second.next_cursor.is_none());

    let wrong_family = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("Registry"),
        0,
        10,
        Some(&cursor),
    )
    .unwrap_err();
    assert!(matches!(
        wrong_family,
        ArtifactServiceError::InvalidInput(_)
    ));

    let mut tampered = cursor.clone().into_bytes();
    let payload_index = tampered.iter().position(|byte| *byte == b'.').unwrap() + 1;
    tampered[payload_index] = if tampered[payload_index] == b'A' {
        b'B'
    } else {
        b'A'
    };
    let tampered = String::from_utf8(tampered).unwrap();
    let error = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        10,
        Some(&tampered),
    )
    .unwrap_err();
    assert!(matches!(error, ArtifactServiceError::InvalidInput(_)));

    drop(register_ready_source(&case_conn, temp.path(), "ds-3"));
    let stale = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        1,
        Some(&cursor),
    )
    .unwrap_err();
    assert!(matches!(stale, ArtifactServiceError::InvalidInput(_)));
}

#[test]
fn case_artifact_cursor_rejects_offset_and_deleted_unconsumed_rows() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db();
    let source_id = DataSourceId("ds-1".to_string());
    let source_conn = crate::source_db::open_source_db(temp.path(), &source_id).unwrap();
    store_artifacts(
        &source_conn,
        &[
            make_artifact_at("newest", "2026-06-03T00:00:00Z"),
            make_artifact_at("middle", "2026-06-02T00:00:00Z"),
            make_artifact_at("oldest", "2026-06-01T00:00:00Z"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    drop(source_conn);

    let case_id = domain::CaseId("case-1".to_string());
    let first = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        1,
        None,
    )
    .unwrap();
    let cursor = first.next_cursor.expect("cursor for remaining rows");

    let offset_error = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        1,
        1,
        Some(&cursor),
    )
    .unwrap_err();
    assert!(matches!(
        offset_error,
        ArtifactServiceError::InvalidInput(_)
    ));

    let source_conn = crate::source_db::open_source_db(temp.path(), &source_id).unwrap();
    source_conn
        .execute("DELETE FROM artifacts WHERE id = 'oldest'", [])
        .unwrap();
    drop(source_conn);

    let stale = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        1,
        Some(&cursor),
    )
    .unwrap_err();
    match stale {
        ArtifactServiceError::InvalidInput(message) => {
            assert!(message.contains("artifact snapshot changed"));
        }
        other => panic!("expected stale cursor validation error, got {other}"),
    }
}

#[test]
fn case_artifact_cursor_rejects_equal_count_replacement() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db();
    let source_id = DataSourceId("ds-1".to_string());
    let source_conn = crate::source_db::open_source_db(temp.path(), &source_id).unwrap();
    let mut originals = vec![
        make_artifact_at("newest", "2026-06-03T00:00:00Z"),
        make_artifact_at("middle", "2026-06-02T00:00:00Z"),
        make_artifact_at("oldest", "2026-06-01T00:00:00Z"),
    ];
    for artifact in &mut originals {
        artifact.source_object_id = Some(FileEntryId("shared-source".to_string()));
        artifact.extractor_id = Some("test.artifact".to_string());
    }
    store_artifacts(&source_conn, &originals, "case-1", "ds-1").unwrap();
    drop(source_conn);

    let case_id = domain::CaseId("case-1".to_string());
    let first = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        1,
        None,
    )
    .unwrap();
    let cursor = first.next_cursor.expect("cursor for remaining rows");

    let source_conn = crate::source_db::open_source_db(temp.path(), &source_id).unwrap();
    let transaction = source_conn.unchecked_transaction().unwrap();
    let repository =
        persistence_sqlite::repositories::artifact_repo::ArtifactRepo::new(&transaction);
    assert_eq!(
        repository
            .delete_analysis_outputs_in_transaction("shared-source", "test.")
            .unwrap(),
        3
    );
    let mut replacements = vec![
        make_artifact_at("replacement-newest", "2026-06-03T00:00:00Z"),
        make_artifact_at("replacement-middle", "2026-06-02T00:00:00Z"),
        make_artifact_at("replacement-oldest", "2026-06-01T00:00:00Z"),
    ];
    for artifact in &mut replacements {
        artifact.source_object_id = Some(FileEntryId("shared-source".to_string()));
        artifact.extractor_id = Some("test.artifact".to_string());
    }
    repository
        .insert_batch_in_transaction(&replacements, "case-1", "ds-1")
        .unwrap();
    transaction.commit().unwrap();

    let stale = get_artifact_rows_page_with_cursor_for_case(
        &case_conn,
        temp.path(),
        &case_id,
        Some("EventLog"),
        0,
        1,
        Some(&cursor),
    )
    .unwrap_err();
    match stale {
        ArtifactServiceError::InvalidInput(message) => {
            assert!(message.contains("artifact snapshot changed"));
        }
        other => panic!("expected stale cursor validation error, got {other}"),
    }
}

#[test]
fn case_artifact_page_refills_each_source_across_multiple_batches() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db();
    let first_source = DataSourceId("ds-1".to_string());
    let first_conn = crate::source_db::open_source_db(temp.path(), &first_source).unwrap();
    let second_conn = register_ready_source(&case_conn, temp.path(), "ds-2");
    let first = (0..300)
        .map(|index| {
            make_artifact_at(
                &format!("first-{index:03}"),
                &format!("2026-06-01T00:{:02}:{:02}Z", index / 60, index % 60),
            )
        })
        .collect::<Vec<_>>();
    let second = (0..300)
        .map(|index| {
            make_artifact_at(
                &format!("second-{index:03}"),
                &format!("2026-06-02T00:{:02}:{:02}Z", index / 60, index % 60),
            )
        })
        .collect::<Vec<_>>();
    store_artifacts(&first_conn, &first, "case-1", "ds-1").unwrap();
    store_artifacts(&second_conn, &second, "case-1", "ds-2").unwrap();
    drop(first_conn);
    drop(second_conn);

    let page = get_artifact_rows_page_for_case(
        &case_conn,
        temp.path(),
        &domain::CaseId("case-1".to_string()),
        None,
        520,
        40,
    )
    .unwrap();

    assert_eq!(page.total, 600);
    assert_eq!(page.items.len(), 40);
    assert!(page.items.iter().all(|row| row.id.starts_with("ds:ds-1:")));
}

#[test]
fn case_lookup_rejects_unscoped_artifact_id() {
    let temp = tempfile::TempDir::new().unwrap();
    let error = get_artifact_row_by_id_for_case(
        &in_memory_case_db(),
        temp.path(),
        &domain::CaseId("case-1".to_string()),
        "artifact-1",
    )
    .unwrap_err();
    assert!(matches!(error, ArtifactServiceError::InvalidInput(_)));
    assert!(error.to_string().contains("ds:<dataSourceId>:<localId>"));
}

#[test]
fn parallel_extraction_filters_before_reading_and_honors_limit() {
    let registry = create_registry();
    let files = vec![
        make_file("txt-1", "/notes/readme.txt"),
        make_file("pf-1", "/Windows/Prefetch/A.EXE-DEADBEEF.pf"),
        make_file("pf-2", "/Windows/Prefetch/B.EXE-DEADBEEF.pf"),
    ];
    let reads = AtomicUsize::new(0);
    let _ = run_extractors_parallel(
        &registry,
        &files,
        |_| {
            reads.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read>)
        },
        1,
    );
    assert_eq!(reads.load(Ordering::Relaxed), 1);
}

#[test]
fn parallel_extraction_reader_error_is_non_fatal() {
    let registry = create_registry();
    let files = vec![make_file("pf-1", "/Windows/Prefetch/CMD.EXE-DEADBEEF.pf")];
    let (artifacts, stats) = run_extractors_parallel(
        &registry,
        &files,
        |_| Err(ArtifactServiceError::other("reader unavailable")),
        10,
    );
    assert!(artifacts.is_empty());
    assert_eq!(stats.warning_count, 1);
    assert_eq!(stats.skipped_count, 1);
}

#[test]
fn parallel_extraction_reads_evidence_serially_before_cpu_work() {
    let registry = create_registry();
    let files = vec![
        make_file("pf-1", "/Windows/Prefetch/A.EXE-DEADBEEF.pf"),
        make_file("pf-2", "/Windows/Prefetch/B.EXE-DEADBEEF.pf"),
        make_file("pf-3", "/Windows/Prefetch/C.EXE-DEADBEEF.pf"),
    ];
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let active_for_reader = Arc::clone(&active);
    let max_for_reader = Arc::clone(&max_active);

    let _ = run_extractors_parallel(
        &registry,
        &files,
        move |_| {
            let current = active_for_reader.fetch_add(1, Ordering::SeqCst) + 1;
            max_for_reader.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            active_for_reader.fetch_sub(1, Ordering::SeqCst);
            Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read>)
        },
        files.len(),
    );

    assert_eq!(max_active.load(Ordering::SeqCst), 1);
}

#[test]
fn parallel_extraction_bounds_prepared_evidence_to_one_batch() {
    let registry = create_registry();
    let files = (0..5)
        .map(|index| {
            make_file(
                &format!("pf-{index}"),
                &format!("/Windows/Prefetch/{index}.EXE-DEADBEEF.pf"),
            )
        })
        .collect::<Vec<_>>();
    let reads = Arc::new(AtomicUsize::new(0));
    let reads_for_thread = Arc::clone(&reads);
    let first_batch_ready = Arc::new(Barrier::new(2));
    let release_first_batch = Arc::new(Barrier::new(2));
    let first_batch_ready_for_thread = Arc::clone(&first_batch_ready);
    let release_first_batch_for_thread = Arc::clone(&release_first_batch);

    let worker = std::thread::spawn(move || {
        run_extractors_parallel(
            &registry,
            &files,
            move |_| {
                let count = reads_for_thread.fetch_add(1, Ordering::SeqCst) + 1;
                if count == extraction::PARALLEL_EXTRACTION_BATCH_SIZE {
                    first_batch_ready_for_thread.wait();
                    release_first_batch_for_thread.wait();
                }
                Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read>)
            },
            files.len(),
        )
    });

    first_batch_ready.wait();
    assert_eq!(
        reads.load(Ordering::SeqCst),
        extraction::PARALLEL_EXTRACTION_BATCH_SIZE
    );
    release_first_batch.wait();
    let _ = worker.join().unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 5);
}

#[test]
fn targeted_scan_skips_unsupported_candidate_without_reading() {
    let conn = in_memory_case_db();
    insert_files(
        &conn,
        &[make_file(
            "evtx-1",
            "Windows/System32/winevt/Logs/System.evtx",
        )],
    );
    let reads = AtomicUsize::new(0);
    let stats = run_targeted_evidence_scan(&conn, "case-1", &["EventLogs"], |_| {
        reads.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read>)
    })
    .unwrap();
    assert_eq!(stats.candidate_count, 1);
    assert_eq!(stats.scanned_count, 0);
    assert_eq!(stats.skipped_count, 1);
    assert_eq!(reads.load(Ordering::Relaxed), 0);
}

#[test]
fn targeted_scan_records_read_error_and_deduplicates_existing_source() {
    let conn = in_memory_case_db();
    insert_files(
        &conn,
        &[make_file("pf-1", "Windows/Prefetch/CMD.EXE-12345678.pf")],
    );
    let stats = run_targeted_evidence_scan(&conn, "case-1", &["ProgramExecution"], |_| {
        Err(ArtifactServiceError::other("reader unavailable"))
    })
    .unwrap();
    assert_eq!(stats.warning_count, 1);
    assert!(stats.warnings[0].contains("reader unavailable"));

    conn.execute(
        "INSERT INTO artifacts
         (id, case_id, data_source_id, artifact_type, source_object_id, title, summary, attrs, created_at)
         VALUES ('artifact-1', 'case-1', 'ds-1', 'Prefetch', 'pf-1', 'Prefetch', 'summary', '{}', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    let reads = AtomicUsize::new(0);
    let stats = run_targeted_evidence_scan(&conn, "case-1", &["ProgramExecution"], |_| {
        reads.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read>)
    })
    .unwrap();
    assert_eq!(stats.skipped_count, 1);
    assert_eq!(reads.load(Ordering::Relaxed), 0);
}
