use super::*;
use evidence_core::{FileSystemDiagnostic, FileSystemDiagnosticKind};

#[test]
fn derived_source_id_is_deterministic_and_path_safe() {
    let id = derived_data_source_id("cluster-123", "16ecc87af5c9").unwrap();

    assert_eq!(id.0, "rbd-cluster-123-16ecc87af5c9");
}

#[test]
fn derived_source_id_rejects_path_and_scope_separators() {
    for invalid in ["../image", "image/name", "image:name", "image\0name"] {
        let error = derived_data_source_id("cluster-123", invalid).unwrap_err();
        assert!(matches!(
            error,
            DerivedSourceError::InvalidIdentity { field: "image ID" }
        ));
    }
}

#[test]
fn derived_source_id_rejects_unbounded_components() {
    let error = derived_data_source_id("cluster-123", &"a".repeat(129)).unwrap_err();

    assert!(matches!(
        error,
        DerivedSourceError::InvalidIdentity { field: "image ID" }
    ));
}

#[test]
fn catalog_manifest_supports_fast_reuse_and_explicit_deep_verification() {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let source = DataSource {
        id: DataSourceId("derived-catalog-test".to_string()),
        name: "VM disk".to_string(),
        kind: DataSourceKind::CephRbd,
        source_path: std::path::PathBuf::from("ceph-rbd://cluster/image"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::default(),
    };
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
        .upsert_source_local_metadata(&CaseId("case-1".to_string()), &source)
        .expect("insert source metadata");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size,
              deleted, hidden, system)
             VALUES
             ('root', NULL, ?1, '', 'root', 'directory', NULL, 0, 0, 0),
             ('other', NULL, ?1, '', 'other', 'directory', NULL, 0, 0, 0),
             ('file', 'root', ?1, 'etc/passwd', 'passwd', 'file', 42, 0, 0, 0)",
            [&source.id.0],
        )
        .expect("insert catalog rows");
    let lineage_fingerprint = "b".repeat(64);
    let summary =
        catalog_manifest::summarize_source_connection(&connection, source.clone()).unwrap();
    catalog_manifest::persist_current_source_manifest(&connection, &lineage_fingerprint, &summary)
        .unwrap();

    connection
        .execute(
            "UPDATE file_entries SET parent_id = 'other' WHERE id = 'file'",
            [],
        )
        .expect("simulate catalog drift");

    let fast = catalog_manifest::load_current_source_summary(
        &connection,
        &lineage_fingerprint,
        source.clone(),
    )
    .unwrap()
    .expect("load persisted manifest");
    assert_eq!(fast.file_count, 3);
    assert!(!catalog_manifest::verify_current_source_manifest_deep(
        &connection,
        &lineage_fingerprint,
        source,
    )
    .unwrap());
}

#[test]
fn derived_catalog_rejects_typed_completeness_diagnostics_only() {
    let mut stats = crate::file_service::EnumerationStats {
        file_count: 1,
        dir_count: 1,
        total_size: 42,
        warnings: vec!["localized metadata warning".to_string()],
        diagnostics: vec![FileSystemDiagnostic::new(
            FileSystemDiagnosticKind::MetadataDegraded,
            "invalid timestamp",
        )],
    };

    super::filesystem::ensure_catalog_complete(&stats)
        .expect("metadata-only diagnostics must not reject a complete catalog");

    stats.diagnostics.push(FileSystemDiagnostic::new(
        FileSystemDiagnosticKind::DirectoryPartial,
        "one directory block was unavailable",
    ));
    let error = super::filesystem::ensure_catalog_complete(&stats)
        .expect_err("typed partial-directory diagnostics must fail closed");
    assert!(matches!(
        error,
        DerivedSourceError::IncompleteCatalog {
            diagnostic_count: 1,
            ..
        }
    ));
}
