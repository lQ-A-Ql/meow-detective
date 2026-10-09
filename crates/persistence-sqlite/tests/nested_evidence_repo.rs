use domain::{DataSourceId, NestedEvidenceLineage};

#[test]
fn lineage_round_trips_proved_range() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE data_sources (id TEXT PRIMARY KEY, case_id TEXT, name TEXT, kind TEXT, source_path TEXT, imported_at TEXT);
        CREATE TABLE nested_evidence_lineage (parent_data_source_id TEXT NOT NULL, nested_file_path TEXT NOT NULL, derived_data_source_id TEXT, offset INTEGER NOT NULL, length INTEGER NOT NULL, probe_kind TEXT NOT NULL, PRIMARY KEY(parent_data_source_id,nested_file_path));").unwrap();
    let value = NestedEvidenceLineage {
        parent_data_source_id: DataSourceId("parent".into()),
        nested_file_path: "disk.vhd".into(),
        derived_data_source_id: None,
        offset: 0,
        length: 4096,
        probe_kind: "vhd-fixed".into(),
    };
    let repo =
        persistence_sqlite::repositories::nested_evidence_repo::NestedEvidenceRepo::new(&conn);
    repo.upsert(&value).unwrap();
    let rows = repo.find_by_parent(&value.parent_data_source_id).unwrap();
    assert_eq!(rows, vec![value]);
}

#[test]
fn lineage_rejects_negative_ranges_in_legacy_schema() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE nested_evidence_lineage (parent_data_source_id TEXT NOT NULL, nested_file_path TEXT NOT NULL, derived_data_source_id TEXT, offset INTEGER NOT NULL, length INTEGER NOT NULL, probe_kind TEXT NOT NULL, PRIMARY KEY(parent_data_source_id,nested_file_path));").unwrap();
    conn.execute(
        "INSERT INTO nested_evidence_lineage VALUES ('parent','disk.vhd',NULL,-1,4096,'vhd-fixed')",
        [],
    )
    .unwrap();
    let error =
        persistence_sqlite::repositories::nested_evidence_repo::NestedEvidenceRepo::new(&conn)
            .find_by_parent(&DataSourceId("parent".into()))
            .unwrap_err();
    assert!(error.to_string().contains("out of range"));
}
