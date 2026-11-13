use super::*;
use persistence_source::fjall::{
    encode_key, instrumentation, CatalogEntryType, KeyPart, PersistMode,
};
use persistence_source::repositories::file_repo::FileRepo;

fn record(id: usize, path: &str) -> FileEntryRecord {
    FileEntryRecord {
        id: format!("record-{id:05}"),
        parent_id: None,
        data_source_id: "source".into(),
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        entry_type: CatalogEntryType::File,
        size: Some(32),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: Some(17),
        unix_mode: None,
    }
}

fn database(records: &[FileEntryRecord]) -> (tempfile::TempDir, Connection) {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/benchmark_pipeline/facts-catalog");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let connection =
        persistence_source::control::open_source_db(root.path(), &DataSourceId("source".into()))
            .unwrap();
    let store = persistence_source::detail::open_store_for_write(&connection)
        .unwrap()
        .unwrap();
    CatalogRecordStore::from_source_store((*store).clone())
        .upsert_batch(records, PersistMode::SyncAll)
        .unwrap();
    (root, connection)
}

#[test]
fn grouped_facts_paths_match_the_existing_queries_with_one_primary_scan() {
    let mut records = (0..1200)
        .map(|id| record(id, &format!("/ordinary/{id}.txt")))
        .collect::<Vec<_>>();
    for (offset, path) in [
        "/etc/hostname",
        "/usr/lib/os-release",
        "/etc/hosts",
        "/boot/vmlinuz-6.1",
        "/etc/kubernetes/manifests/api.yaml",
        "/etc/kubernetes/kubelet.conf",
        "/var/lib/docker/containers/a/config.v2.json",
        "/etc/hostname-os-release",
        "/ETC/HOSTNAME",
        "/etc/hostname.deleted",
    ]
    .iter()
    .enumerate()
    {
        records.push(record(1200 + offset, path));
    }
    records.last_mut().unwrap().deleted = true;
    let (_root, connection) = database(&records);
    let source = DataSourceId("source".into());
    let repository = FileRepo::new(&connection);
    let mut expected = GROUPS
        .iter()
        .flat_map(|group| repository.find_by_path_fragments(&source, group).unwrap())
        .collect::<Vec<_>>();
    expected.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    expected.dedup_by(|left, right| left.id == right.id);
    let _open = persistence_source::detail::require_source_store(&connection).unwrap();
    instrumentation::enable();
    instrumentation::reset();
    let actual = collect_entries(&connection, &source).unwrap();
    let counters = instrumentation::snapshot();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let pages = records.len().div_ceil(PAGE_SIZE) as u64;
    assert_eq!(counters.point_reads, 0);
    assert_eq!(counters.scan_calls, pages);
    assert!(counters.scan_entries <= records.len() as u64 + pages);
}

#[test]
fn an_overflowing_group_is_discarded_without_discarding_other_groups() {
    let records = [
        record(0, "/etc/hostname.0"),
        record(1, "/etc/hostname.1"),
        record(2, "/etc/hostname.2"),
        record(3, "/etc/hosts"),
        record(4, "/etc/hostname-os-release"),
    ];
    let (_root, connection) = database(&records);
    let actual = read_entries(&connection, &DataSourceId("source".into()), 2).unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["/etc/hostname-os-release", "/etc/hosts"]
    );
}

#[test]
fn a_foreign_source_returns_a_catalog_error() {
    let (_root, connection) = database(&[record(0, "/etc/hostname")]);
    assert!(collect_entries(&connection, &DataSourceId("foreign".into())).is_err());
}

#[test]
fn catalog_scan_preserves_read_cancellation_and_timeout() {
    use persistence_sqlite::read_operation::ReadCancellation;
    use std::sync::Arc;

    let (_root, connection) = database(&[record(0, "/etc/hostname")]);
    for timed_out in [false, true] {
        let control = Arc::new(ReadCancellation::default());
        let _scope = control.enter();
        if timed_out {
            control.time_out();
        } else {
            control.cancel();
        }
        let result = collect_entries(&connection, &DataSourceId("source".into()));
        if timed_out {
            assert!(matches!(result, Err(DbError::ReadTimedOut)));
        } else {
            assert!(matches!(result, Err(DbError::ReadCancelled)));
        }
    }
}

#[test]
fn corrupt_catalog_records_produce_visible_facts_diagnostics() {
    let (root, source_connection) = database(&[record(0, "/etc/hostname")]);
    let case_connection = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&case_connection).unwrap();
    let case_id = domain::CaseId("facts-case".into());
    let now = chrono::Utc::now();
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&case_connection)
        .create(&domain::CaseMeta {
            id: case_id.clone(),
            name: "Facts corruption".into(),
            number: None,
            examiner: None,
            notes: None,
            created_at: now,
            updated_at: now,
        })
        .unwrap();
    let source_id = DataSourceId("source".into());
    let evidence = root.path().join("evidence");
    std::fs::create_dir_all(evidence.join("etc")).unwrap();
    std::fs::write(evidence.join("etc/hostname"), b"fixture-host\n").unwrap();
    let source = domain::DataSource {
        id: source_id.clone(),
        name: "Facts source".into(),
        kind: domain::DataSourceKind::LogicalDirectory,
        source_path: evidence,
        imported_at: now,
        provenance: domain::DataSourceProvenance::unknown(),
    };
    let mut storage =
        persistence_sqlite::repositories::datasource_repo::DataSourceStorage::source_db(
            &source_id.0,
            Some("linux"),
            None,
        );
    storage.import_state = "ready".into();
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&case_connection)
        .insert_with_storage(&case_id, &source, &storage)
        .unwrap();
    let store = persistence_source::detail::open_store_for_write(&source_connection)
        .unwrap()
        .unwrap();
    let prefix = encode_key("catalog.record", &[KeyPart::Text(source_id.0.clone())]).unwrap();
    let entries = store
        .database()
        .read_snapshot()
        .keyset(
            persistence_source::fjall::KeyspaceName::CatalogRecords,
            &prefix,
            None,
            ScanDirection::Forward,
            1,
        )
        .unwrap();
    store
        .write_raw(
            persistence_source::fjall::KeyspaceName::CatalogRecords,
            entries[0].key.clone(),
            b"damaged catalog envelope".to_vec(),
            PersistMode::SyncAll,
        )
        .unwrap();
    let facts = super::super::collect_linux_evidence_facts(
        &case_connection,
        root.path(),
        &case_id,
        &source_id,
    );
    assert!(facts
        .diagnostics
        .iter()
        .any(|message| message.contains("source catalog query failed")));
    assert!(facts.hostname.is_none());
}
