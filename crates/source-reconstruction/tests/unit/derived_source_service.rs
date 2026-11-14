use super::*;
crate::native_source_test_support!();
use domain::{CaseId, DataSourceKind, EntryType, FileEntry, FileEntryId};
use evidence_core::{FileSystemDiagnostic, FileSystemDiagnosticKind};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use test_support::NativeSourceFixture;

#[test]
fn derived_source_id_is_deterministic_and_path_safe() {
    let id = derived_data_source_id("scope:ceph:123", "16ecc87af5c9").unwrap();

    assert_eq!(id.0, "rbd-12d81d799fc8dc4820ec-16ecc87af5c9");
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
fn pre_cancelled_materialization_stops_before_database_access() {
    let cancel_token = Arc::new(AtomicBool::new(false));
    cancel_token.store(true, Ordering::Relaxed);
    let connection = rusqlite::Connection::open_in_memory().expect("open empty database");

    let error = materialize_rbd_sources_for_scope_with_cancel(
        &connection,
        std::path::Path::new("."),
        &CaseId("case-cancelled".to_string()),
        &domain::CephScopeId("scope:ceph:cancelled".to_string()),
        cancel_token,
    )
    .expect_err("pre-cancelled materialization must stop before querying the database");

    assert!(matches!(error, DerivedSourceError::ProcessingCancelled));
}

#[test]
fn rbd_materialization_rejects_a_non_ceph_scope() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO cases (id, name) VALUES ('case-topology-kind', 'case')",
            [],
        )
        .unwrap();
    connection.execute(
        "INSERT INTO linux_topology_scopes (
            id, case_id, scope_kind, name, identity_state, status,
            evidence_completeness, diagnostics_json
         ) VALUES ('scope:pve:1', 'case-topology-kind', 'pve', 'PVE', 'candidate', 'ready', 'complete', '[]')",
        [],
    ).unwrap();
    let error = materialize_rbd_sources_for_scope(
        &connection,
        std::path::Path::new("."),
        &CaseId("case-topology-kind".to_string()),
        &domain::CephScopeId("scope:pve:1".to_string()),
    )
    .unwrap_err();
    assert!(matches!(error, DerivedSourceError::ScopeNotFound(ref id) if id == "scope:pve:1"));
}

#[test]
fn rbd_materialization_rejects_a_kubernetes_scope() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO cases (id, name) VALUES ('case-kubernetes-kind', 'case')",
            [],
        )
        .unwrap();
    connection.execute(
        "INSERT INTO linux_topology_scopes (
            id, case_id, scope_kind, name, identity_state, status,
            evidence_completeness, diagnostics_json
         ) VALUES ('scope:kubernetes:1', 'case-kubernetes-kind', 'kubernetes', 'Kubernetes', 'candidate', 'ready', 'complete', '[]')",
        [],
    ).unwrap();
    let error = materialize_rbd_sources_for_scope(
        &connection,
        std::path::Path::new("."),
        &CaseId("case-kubernetes-kind".to_string()),
        &domain::CephScopeId("scope:kubernetes:1".to_string()),
    )
    .unwrap_err();
    assert!(
        matches!(error, DerivedSourceError::ScopeNotFound(ref id) if id == "scope:kubernetes:1")
    );
}

#[test]
fn rbd_materialization_rejects_a_missing_scope_before_creating_output() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("derived-must-not-exist");

    let error = materialize_rbd_sources_for_scope(
        &connection,
        &output,
        &CaseId("case-missing-scope".into()),
        &domain::CephScopeId("scope:ceph:missing".into()),
    )
    .unwrap_err();

    assert!(
        matches!(error, DerivedSourceError::ScopeNotFound(ref id) if id == "scope:ceph:missing")
    );
    assert!(!output.exists());
    let sources: i64 = connection
        .query_row("SELECT COUNT(*) FROM data_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sources, 0);
}

#[test]
fn rbd_materialization_rejects_a_foreign_case_scope_before_creating_output() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO cases (id, name) VALUES ('case-owner', 'owner')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO linux_topology_scopes (
                id, case_id, scope_kind, name, identity_state, status,
                evidence_completeness, diagnostics_json
             ) VALUES ('scope:ceph:foreign', 'case-owner', 'ceph', 'Ceph',
                'proven', 'ready', 'complete', '[]')",
            [],
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("derived-must-not-exist");

    let error = materialize_rbd_sources_for_scope(
        &connection,
        &output,
        &CaseId("case-other".into()),
        &domain::CephScopeId("scope:ceph:foreign".into()),
    )
    .unwrap_err();

    assert!(
        matches!(error, DerivedSourceError::ScopeNotFound(ref id) if id == "scope:ceph:foreign")
    );
    assert!(!output.exists());
    let sources: i64 = connection
        .query_row("SELECT COUNT(*) FROM data_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sources, 0);
}

#[test]
fn rbd_materialization_preserves_database_failure_in_scope_validation() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let error = materialize_rbd_sources_for_scope(
        &connection,
        std::path::Path::new("."),
        &CaseId("case-database-failure".into()),
        &domain::CephScopeId("scope:ceph:database-failure".into()),
    )
    .unwrap_err();

    assert!(matches!(error, DerivedSourceError::Database(_)));
}

#[test]
fn catalog_manifest_supports_fast_reuse_and_explicit_deep_verification() {
    let temp = tempfile::tempdir().expect("create case root");
    let source = DataSource {
        id: DataSourceId("derived-catalog-test".to_string()),
        name: "VM disk".to_string(),
        kind: DataSourceKind::CephRbd,
        source_path: std::path::PathBuf::from("ceph-rbd://cluster/image"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::default(),
    };
    let connection = source_kernel::source_db::open_source_db(temp.path(), &source.id)
        .expect("open source database");
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
        .upsert_source_local_metadata(&CaseId("case-1".to_string()), &source)
        .expect("insert source metadata");
    persistence_source::repositories::file_repo::FileRepo::new(&connection)
        .insert_batch(&[
            catalog_dir(&source, "root", None, "", "root"),
            catalog_dir(&source, "other", None, "", "other"),
            catalog_file(&source, "file", "root", "etc/passwd", "passwd"),
        ])
        .expect("insert catalog rows");
    let lineage_fingerprint = "b".repeat(64);
    let summary =
        catalog_manifest::summarize_source_connection(&connection, source.clone()).unwrap();
    let fjall_source =
        source_kernel::source_db::open_fjall_source_for_build(&connection, &source.id)
            .expect("open Fjall source storage");
    fjall_source
        .partition_projection_publish(&source.id.0)
        .expect("publish Fjall partition projection");
    catalog_manifest::persist_current_source_manifest(
        &connection,
        &fjall_source,
        &lineage_fingerprint,
        &summary,
    )
    .unwrap();

    persistence_source::detail::update_catalog_parent(
        &connection,
        "file",
        Some("other".into()),
        &source.id.0,
    )
    .unwrap();

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
fn catalog_digest_ignores_materialized_partition_routing_hints() {
    let fixture = NativeSourceFixture::new("derived-partition-hint-test", "case-1");
    let source = DataSource {
        id: DataSourceId("derived-partition-hint-test".to_string()),
        name: "VM disk".to_string(),
        kind: DataSourceKind::CephRbd,
        source_path: std::path::PathBuf::from("ceph-rbd://cluster/image"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::default(),
    };
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&fixture.connection)
        .upsert_source_local_metadata(&CaseId("case-1".to_string()), &source)
        .expect("insert source metadata");
    persistence_source::repositories::file_repo::FileRepo::new(&fixture.connection)
        .insert_batch(&[
            catalog_dir(&source, "root-1", None, "", "Partition 1"),
            catalog_file(&source, "file-1", "root-1", "etc/passwd", "passwd"),
            catalog_dir(&source, "root-0", None, "", "Partition 0"),
            catalog_file(&source, "file-0", "root-0", "boot/grub.cfg", "grub.cfg"),
        ])
        .expect("insert catalog rows");
    fixture.seal();

    let before =
        catalog_manifest::summarize_source_connection(&fixture.connection, source.clone()).unwrap();
    for id in ["root-0", "file-0"] {
        persistence_source::detail::set_partition_index(&fixture.connection, id, 0).unwrap();
    }
    for id in ["root-1", "file-1"] {
        persistence_source::detail::set_partition_index(&fixture.connection, id, 1).unwrap();
    }
    let after = catalog_manifest::summarize_source_connection(&fixture.connection, source).unwrap();

    assert_eq!(before.file_count, after.file_count);
    assert_eq!(before.catalog_digest, after.catalog_digest);
}

fn catalog_dir(
    source: &DataSource,
    id: &str,
    parent_id: Option<&str>,
    path: &str,
    name: &str,
) -> FileEntry {
    catalog_entry(
        source,
        id,
        parent_id,
        path,
        name,
        EntryType::Directory,
        None,
    )
}

fn catalog_file(
    source: &DataSource,
    id: &str,
    parent_id: &str,
    path: &str,
    name: &str,
) -> FileEntry {
    catalog_entry(
        source,
        id,
        Some(parent_id),
        path,
        name,
        EntryType::File,
        Some(42),
    )
}

fn catalog_entry(
    source: &DataSource,
    id: &str,
    parent_id: Option<&str>,
    path: &str,
    name: &str,
    entry_type: EntryType,
    size: Option<u64>,
) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.into()),
        parent_id: parent_id.map(|value| FileEntryId(value.into())),
        data_source_id: source.id.clone(),
        path: path.into(),
        name: name.into(),
        entry_type,
        size,
        ext: name.rsplit_once('.').map(|(_, ext)| ext.into()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

#[test]
fn derived_catalog_rejects_typed_completeness_diagnostics_only() {
    let mut stats = evidence_access::file_service::EnumerationStats {
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
