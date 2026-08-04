use super::*;

fn setup_db() -> rusqlite::Connection {
    let conn = crate::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            number TEXT,
            examiner TEXT,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE data_source_clusters (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            root_path TEXT NOT NULL,
            platform TEXT NOT NULL DEFAULT 'linux',
            profile TEXT,
            manifest_rel_path TEXT NOT NULL,
            import_state TEXT NOT NULL DEFAULT 'pending',
            member_count INTEGER NOT NULL DEFAULT 0,
            ready_count INTEGER NOT NULL DEFAULT 0,
            failed_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE data_sources (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id),
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            source_path TEXT NOT NULL,
            imported_at TEXT NOT NULL DEFAULT (datetime('now')),
            source_hash_sha256 TEXT,
            hash_status TEXT DEFAULT 'unknown',
            canonical_source_path TEXT,
            evidence_size INTEGER,
            reader_kind TEXT,
            provenance_status TEXT DEFAULT 'unknown',
            provenance_warnings TEXT DEFAULT '[]',
            storage_model TEXT NOT NULL DEFAULT 'source_db',
            source_db_rel_path TEXT,
            index_rel_path TEXT,
            staging_rel_path TEXT,
            platform TEXT NOT NULL DEFAULT 'unknown',
            profile TEXT,
            import_state TEXT NOT NULL DEFAULT 'pending',
            schema_version TEXT,
            last_error TEXT,
            cluster_id TEXT REFERENCES data_source_clusters(id) ON DELETE SET NULL,
            cluster_member_index INTEGER,
            cluster_member_count INTEGER
        );
        CREATE TABLE file_entries (
            id TEXT PRIMARY KEY NOT NULL,
            parent_id TEXT,
            data_source_id TEXT NOT NULL,
            path TEXT NOT NULL,
            name TEXT NOT NULL,
            entry_type TEXT NOT NULL,
            size INTEGER,
            ext TEXT,
            deleted INTEGER NOT NULL DEFAULT 0,
            created_at TEXT,
            modified_at TEXT,
            accessed_at TEXT,
            changed_at TEXT,
            hash_sha256 TEXT
        );
        CREATE TABLE artifacts (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL DEFAULT '',
            data_source_id TEXT NOT NULL DEFAULT '',
            artifact_type TEXT NOT NULL,
            source_object_id TEXT,
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            attrs TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE timeline_events (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL DEFAULT '',
            source_object_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            ts TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            attrs TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE data_source_partitions (
            id TEXT PRIMARY KEY,
            data_source_id TEXT NOT NULL,
            partition_index INTEGER NOT NULL,
            name TEXT NOT NULL,
            kind_label TEXT NOT NULL,
            status TEXT NOT NULL,
            type_guid TEXT,
            offset INTEGER NOT NULL,
            length INTEGER NOT NULL,
            filesystem TEXT,
            unlock_hint TEXT,
            lvm_vg_uuid TEXT,
            lvm_vg_name TEXT,
            lvm_lv_uuid TEXT,
            lvm_lv_name TEXT,
            lvm_pv_offsets_json TEXT,
            lvm_pv_sources_json TEXT
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
        params!["case-1", "Test Case"],
    ).unwrap();
    conn
}

fn make_ds(id: &str, name: &str) -> DataSource {
    DataSource {
        id: DataSourceId(id.to_string()),
        name: name.to_string(),
        kind: DataSourceKind::Raw,
        source_path: std::path::PathBuf::from("/evidence/image.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    }
}

#[test]
fn insert_then_find_by_case_returns_it() {
    let conn = setup_db();
    let repo = DataSourceRepo::new(&conn);
    let ds = make_ds("ds-1", "Disk Image");
    repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();
    let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Disk Image");
    assert_eq!(results[0].kind, DataSourceKind::Raw);
}

#[test]
fn ceph_rbd_kind_round_trips_without_raw_downgrade() {
    let conn = setup_db();
    let repo = DataSourceRepo::new(&conn);
    let mut ds = make_ds("ds-ceph-rbd", "Derived VM disk");
    ds.kind = DataSourceKind::CephRbd;

    repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

    let stored = repo
        .find_by_case(&CaseId("case-1".to_string()))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(stored.kind, DataSourceKind::CephRbd);
    assert_eq!(
        repo.source_kind(&DataSourceId("ds-ceph-rbd".to_string()))
            .unwrap(),
        DataSourceKind::CephRbd
    );
    let raw_kind: String = conn
        .query_row(
            "SELECT kind FROM data_sources WHERE id = 'ds-ceph-rbd'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(raw_kind, "ceph_rbd");
}

#[test]
fn insert_then_find_by_case_round_trips_provenance() {
    let conn = setup_db();
    let repo = DataSourceRepo::new(&conn);
    let mut ds = make_ds("ds-1", "Disk Image");
    ds.provenance = DataSourceProvenance {
        source_hash_sha256: Some("a".repeat(64)),
        hash_status: DataSourceHashStatus::Hashed,
        canonical_source_path: Some(std::path::PathBuf::from("/canonical/image.E01")),
        evidence_size: Some(42_000),
        reader_kind: Some("raw-image".to_string()),
        provenance_status: DataSourceProvenanceStatus::Recorded,
        warnings: vec![
            "sparse image metadata".to_string(),
            "hash verified".to_string(),
        ],
    };
    repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();
    let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].provenance, ds.provenance);
}

#[test]
fn mount_source_identity_queries_round_trip_registered_evidence() {
    let conn = setup_db();
    let repo = DataSourceRepo::new(&conn);
    let mut ds = make_ds("ds-mount", "Mount source");
    ds.kind = DataSourceKind::E01;
    ds.source_path = std::path::PathBuf::from("D:/evidence/sample.E01");
    ds.provenance.source_hash_sha256 = Some("a".repeat(64));
    ds.provenance.evidence_size = Some(42_000);
    repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

    assert_eq!(repo.source_path(&ds.id).unwrap(), "D:/evidence/sample.E01");
    assert_eq!(
        repo.source_fingerprint(&ds.id).unwrap(),
        ds.provenance.source_hash_sha256
    );
    assert_eq!(repo.source_kind(&ds.id).unwrap(), DataSourceKind::E01);
    assert_eq!(repo.source_evidence_size(&ds.id).unwrap(), Some(42_000));
}

#[test]
fn legacy_null_provenance_loads_safe_defaults() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at, source_hash_sha256,
            hash_status, canonical_source_path, evidence_size, reader_kind,
            provenance_status, provenance_warnings
        ) VALUES (
            'legacy-ds', 'case-1', 'Legacy', 'raw', '/legacy.raw',
            '2026-01-01T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, NULL
        )",
        [],
    )
    .unwrap();
    let results = DataSourceRepo::new(&conn)
        .find_by_case(&CaseId("case-1".to_string()))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].provenance, DataSourceProvenance::unknown());
}

#[test]
fn rename_changes_the_name() {
    let conn = setup_db();
    let repo = DataSourceRepo::new(&conn);
    let ds = make_ds("ds-1", "Old Name");
    repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();
    repo.rename(&DataSourceId("ds-1".to_string()), "New Name")
        .unwrap();
    let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
    assert_eq!(results[0].name, "New Name");
}

#[test]
fn update_cluster_membership_requires_existing_data_source() {
    let conn = setup_db();
    let repo = DataSourceRepo::new(&conn);
    let ds = make_ds("ds-1", "Disk Image");
    repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();
    conn.execute(
        "INSERT INTO data_source_clusters
            (id, case_id, name, root_path, manifest_rel_path)
         VALUES
            ('cluster-1', 'case-1', 'PVE cluster', 'cluster-root', 'cluster.json')",
        [],
    )
    .unwrap();
    repo.update_cluster_membership(&DataSourceId("ds-1".to_string()), "cluster-1", 1, 3)
        .unwrap();

    let stored: (String, i64, i64) = conn
        .query_row(
            "SELECT cluster_id, cluster_member_index, cluster_member_count
             FROM data_sources WHERE id = 'ds-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored, ("cluster-1".to_string(), 1, 3));

    let error =
        repo.update_cluster_membership(&DataSourceId("missing".to_string()), "cluster-1", 0, 3);
    assert!(error.is_err());
}
