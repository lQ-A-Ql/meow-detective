use domain::{DataSourceId, NestedEvidenceLineage};

#[test]
fn lineage_round_trips_proved_range() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE data_sources (id TEXT PRIMARY KEY, case_id TEXT, name TEXT, kind TEXT, source_path TEXT, imported_at TEXT);
        CREATE TABLE nested_evidence_lineage (parent_data_source_id TEXT NOT NULL, nested_file_path TEXT NOT NULL, parent_file_id TEXT, derived_data_source_id TEXT, offset INTEGER NOT NULL, length INTEGER NOT NULL, probe_kind TEXT NOT NULL, PRIMARY KEY(parent_data_source_id,nested_file_path));").unwrap();
    let value = NestedEvidenceLineage {
        parent_data_source_id: DataSourceId("parent".into()),
        nested_file_path: "disk.vhd".into(),
        parent_file_id: Some("file-1".into()),
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
    conn.execute_batch("CREATE TABLE nested_evidence_lineage (parent_data_source_id TEXT NOT NULL, nested_file_path TEXT NOT NULL, parent_file_id TEXT, derived_data_source_id TEXT, offset INTEGER NOT NULL, length INTEGER NOT NULL, probe_kind TEXT NOT NULL, PRIMARY KEY(parent_data_source_id,nested_file_path));").unwrap();
    for (offset, length, column) in [(-1, 4096, 4), (0, -1, 5)] {
        conn.execute(
            "INSERT OR REPLACE INTO nested_evidence_lineage(parent_data_source_id,nested_file_path,parent_file_id,derived_data_source_id,offset,length,probe_kind) VALUES ('parent','disk.vhd',NULL,NULL,?1,?2,'vhd-fixed')",
            [offset, length],
        )
        .unwrap();
        let error =
            persistence_sqlite::repositories::nested_evidence_repo::NestedEvidenceRepo::new(&conn)
                .find_by_parent(&DataSourceId("parent".into()))
                .unwrap_err();
        assert!(matches!(
            error,
            persistence_sqlite::connection::DbError::Sqlite(
                rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Integer, source)
            ) if index == column && source.is::<std::num::TryFromIntError>()
        ));
    }
}
