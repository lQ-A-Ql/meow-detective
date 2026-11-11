use domain::{DataSourceId, FileEntryId};
use persistence_source::detail::configure_source_identity;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, file_repo::FileRepo, timeline_repo::TimelineRepo,
};
use tempfile::TempDir;

#[test]
fn source_detail_repositories_fail_closed_without_fjall_storage() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&connection).unwrap();
    configure_source_identity(&connection, "source-1").unwrap();

    let source = DataSourceId("source-1".into());
    let repository = FileRepo::new(&connection);

    let error = repository.index_page(&source.0, "", 10).unwrap_err();
    assert!(error
        .to_string()
        .contains("source detail storage is missing"));

    let error = repository
        .registry_hive_candidates(&source, "", 10)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("source detail storage is missing"));

    let error = repository
        .visit_all(Some(&source.0), |_| Ok(()))
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("source detail storage is missing"));

    let error = repository.find_root_for_partition(&source, 0).unwrap_err();
    assert!(error
        .to_string()
        .contains("source detail storage is missing"));

    let error = repository
        .find_mount_children_for_partition(&FileEntryId("root".into()), &source, 0)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("source detail storage is missing"));

    let error = ArtifactRepo::new(&connection).count().unwrap_err();
    assert!(error
        .to_string()
        .contains("source detail storage is missing"));

    let error = TimelineRepo::new(&connection).count().unwrap_err();
    assert!(error
        .to_string()
        .contains("source detail storage is missing"));
}

#[test]
fn configured_source_never_materializes_a_missing_generation_during_reads() {
    let temporary = TempDir::new().unwrap();
    let database_path = temporary.path().join("source.db");
    let connection = persistence_sqlite::open_or_create_source(&database_path).unwrap();
    configure_source_identity(&connection, "source-1").unwrap();
    persistence_source::detail::configure_source_fjall(&connection, &database_path).unwrap();

    let error = ArtifactRepo::new(&connection).count().unwrap_err();
    assert!(error
        .to_string()
        .contains("source generation directory is missing"));
    assert!(!temporary.path().join("source.fjall").exists());
}
