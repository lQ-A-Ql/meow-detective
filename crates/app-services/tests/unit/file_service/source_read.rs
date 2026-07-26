use super::*;
use domain::FileEntryId;
use tempfile::TempDir;

#[test]
fn descriptor_cache_is_bounded_for_large_analysis_scans() {
    let mut cache = HashMap::new();
    for index in 0..=MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES {
        cache_preview_descriptor(
            &mut cache,
            &format!("file-{index}"),
            &serde_json::json!({"fileId": index}),
        );
    }

    assert_eq!(cache.len(), 1);
    assert!(cache.contains_key(&format!("file-{MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES}")));
}

fn insert_source(
    connection: &rusqlite::Connection,
    source_id: &str,
    kind: &str,
    source_path: &str,
) {
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, 'case-1', 'source', ?2, ?3, '2026-07-18T00:00:00Z')",
            rusqlite::params![source_id, kind, source_path],
        )
        .expect("insert source");
}

fn source_read_hint(
    source_id: &str,
    file_id: &str,
    partition_index: Option<usize>,
    path: &str,
) -> SourceReadFileHint {
    SourceReadFileHint::new(
        FileEntryId(file_id.to_string()),
        DataSourceId(source_id.to_string()),
        partition_index,
        path.to_string(),
        16,
        false,
    )
}

#[test]
fn partition_metadata_fast_path_builds_and_caches_descriptor_without_file_entry_lookup() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    insert_source(&source_conn, "derived-linux", "ceph_rbd", "derived.rbd");
    source_conn
        .execute(
            "INSERT INTO data_source_partitions
             (id, data_source_id, partition_index, name, kind_label, status,
              offset, length, filesystem)
             VALUES ('partition-2', 'derived-linux', 2, 'Root', 'XFS', 'ready',
                     0, 4096, 'XFS')",
            [],
        )
        .expect("insert partition");
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_id = DataSourceId("derived-linux".to_string());
    let mut context = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_id,
    );

    let first = context
        .descriptor_for_hint(
            &source_read_hint(
                "derived-linux",
                "not-present-in-file-entries",
                Some(2),
                "var/www/a.php",
            ),
            2,
            "ceph_rbd".to_string(),
            "derived.rbd".to_string(),
        )
        .expect("build direct descriptor");
    source_conn
        .execute("DELETE FROM data_source_partitions", [])
        .expect("remove persisted partitions after cache fill");
    source_conn
        .execute("DELETE FROM data_sources", [])
        .expect("remove source location after cache fill");
    let second = context
        .descriptor_for_hint(
            &source_read_hint(
                "derived-linux",
                "also-not-present-in-file-entries",
                Some(2),
                "var/www/b.php",
            ),
            2,
            "ceph_rbd".to_string(),
            "derived.rbd".to_string(),
        )
        .expect("reuse source-level metadata caches");

    assert_eq!(first.partition_candidates.len(), 1);
    assert_eq!(first.partition_candidates[0].partition_index, 2);
    assert_eq!(second.partition_candidates, first.partition_candidates);
}

#[test]
fn null_partition_candidate_falls_back_to_existing_file_id_reader() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    let evidence = TempDir::new().expect("create logical evidence");
    std::fs::write(evidence.path().join("fallback.txt"), b"fallback-by-id")
        .expect("write logical evidence");
    insert_source(
        &source_conn,
        "logical-linux",
        "logical_directory",
        evidence.path().to_string_lossy().as_ref(),
    );
    source_conn
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, deleted, hidden, system,
              encrypted, partition_index)
             VALUES ('fallback-file', 'logical-linux', 'fallback.txt', 'fallback.txt',
                     'file', 14, 0, 0, 0, 0, NULL)",
            [],
        )
        .expect("insert fallback file entry");
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_id = DataSourceId("logical-linux".to_string());
    let mut context = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_id,
    );

    let bytes = context
        .read_file_header_with_metadata(
            SourceReadFileHint::new(
                FileEntryId("fallback-file".to_string()),
                DataSourceId("logical-linux".to_string()),
                None,
                "ignored-candidate-path".to_string(),
                16,
                false,
            ),
            64,
        )
        .expect("read through file-id fallback");

    assert_eq!(bytes, b"fallback-by-id");
}

#[test]
fn encrypted_metadata_hint_is_rejected_before_derived_runtime_initialization() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    insert_source(
        &source_conn,
        "derived-linux",
        "ceph_rbd",
        "missing-derived.rbd",
    );
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_id = DataSourceId("derived-linux".to_string());
    let mut context = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_id,
    );

    let error = context
        .read_file_header_with_metadata(
            SourceReadFileHint::new(
                FileEntryId("encrypted-rbd-file".to_string()),
                source_id.clone(),
                Some(2),
                "Windows/System32/config/SYSTEM".to_string(),
                4096,
                true,
            ),
            4096,
        )
        .expect_err("EFS metadata hint must fail before opening the RBD provider");

    assert!(matches!(error, FileServiceError::Unsupported(_)));
    assert!(error.to_string().contains("EFS-encrypted"));
    assert!(!error.to_string().contains("Windows/System32/config/SYSTEM"));
    assert!(!error.to_string().contains("missing-derived.rbd"));
    assert!(context.derived_runtime.is_none());
}

#[test]
fn source_read_hint_preserves_encrypted_file_fact() {
    let hint = SourceReadFileHint::new(
        FileEntryId("encrypted-rbd-file".to_string()),
        DataSourceId("derived-linux".to_string()),
        Some(2),
        "Windows/System32/config/SYSTEM".to_string(),
        4096,
        true,
    );

    assert!(hint.encrypted);
}

#[test]
fn partition_candidate_cache_is_bounded() {
    let mut cache = HashMap::new();
    for partition_index in 0..=MAX_SOURCE_PARTITION_CACHE_ENTRIES {
        cache_partition_candidates(&mut cache, partition_index, Vec::new());
    }

    assert_eq!(cache.len(), 1);
    assert!(cache.contains_key(&MAX_SOURCE_PARTITION_CACHE_ENTRIES));
}
