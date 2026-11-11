use super::{bind_source_view, ResumeIdentityError, SOURCE_VIEW_KEY};
use crate::source_runtime::open_factory;
use domain::{DataSourceId, DataSourceKind, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo;

fn setup() -> (tempfile::TempDir, rusqlite::Connection, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, [0; 512]).unwrap();
    let database_path = temp.path().join("source.db");
    let conn = persistence_sqlite::open_or_create_source(&database_path).unwrap();
    persistence_source::detail::configure_source_identity(&conn, "source").unwrap();
    persistence_source::detail::configure_source_fjall(&conn, &database_path).unwrap();
    (temp, conn, path)
}

#[test]
fn metadata_bound_work_reopens_only_with_the_same_member_view() {
    let (temp, conn, path) = setup();
    let source = DataSourceId("source".into());
    let factory = open_factory(&path, &DataSourceKind::Raw).unwrap();
    bind_source_view(&conn, &source, factory.as_ref(), temp.path(), true).unwrap();
    let identity = SourceMetaRepo::new(&conn)
        .read(SOURCE_VIEW_KEY)
        .unwrap()
        .unwrap();
    assert!(identity.starts_with("metadata-v1:"));
    drop(factory);
    let same = open_factory(&path, &DataSourceKind::Raw).unwrap();
    bind_source_view(&conn, &source, same.as_ref(), temp.path(), false).unwrap();
    std::fs::write(&path, [0; 1024]).unwrap();
    let changed = open_factory(&path, &DataSourceKind::Raw).unwrap();
    assert!(matches!(
        bind_source_view(&conn, &source, changed.as_ref(), temp.path(), false),
        Err(ResumeIdentityError::Changed)
    ));
    assert_eq!(
        SourceMetaRepo::new(&conn)
            .read(SOURCE_VIEW_KEY)
            .unwrap()
            .unwrap(),
        identity
    );
}

#[test]
fn unbound_existing_staging_is_never_adopted_as_a_verified_snapshot() {
    let (temp, conn, path) = setup();
    let staging = crate::staging::staging_dir(temp.path(), "source");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(
        staging.join(infrastructure::constants::MANIFEST_FILE_NAME),
        b"{interrupted",
    )
    .unwrap();
    assert!(crate::staging::StagingManifest::load(temp.path(), "source").is_none());
    let factory = open_factory(&path, &DataSourceKind::Raw).unwrap();
    assert!(matches!(
        bind_source_view(
            &conn,
            &DataSourceId("source".into()),
            factory.as_ref(),
            temp.path(),
            true
        ),
        Err(ResumeIdentityError::Unbound)
    ));
    assert!(SourceMetaRepo::new(&conn)
        .read(SOURCE_VIEW_KEY)
        .unwrap()
        .is_none());
}

#[test]
fn nonempty_unbound_catalog_cannot_bypass_the_staging_identity_check() {
    let (temp, conn, path) = setup();
    persistence_source::repositories::file_repo::FileRepo::new(&conn)
        .insert_batch(&[
            FileEntry {
                id: FileEntryId("root".into()),
                parent_id: None,
                data_source_id: DataSourceId("source".into()),
                path: String::new(),
                name: String::new(),
                entry_type: EntryType::Directory,
                size: None,
                ext: None,
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
            },
            FileEntry {
                id: FileEntryId("file".into()),
                parent_id: Some(FileEntryId("root".into())),
                data_source_id: DataSourceId("source".into()),
                path: "a".into(),
                name: "a".into(),
                entry_type: EntryType::File,
                size: Some(0),
                ext: None,
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
            },
        ])
        .unwrap();
    let factory = open_factory(&path, &DataSourceKind::Raw).unwrap();
    assert!(matches!(
        bind_source_view(
            &conn,
            &DataSourceId("source".into()),
            factory.as_ref(),
            temp.path(),
            true
        ),
        Err(ResumeIdentityError::Unbound)
    ));
}

#[test]
fn an_empty_previously_started_source_does_not_bless_missing_view_metadata() {
    let (temp, conn, path) = setup();
    let factory = open_factory(&path, &DataSourceKind::Raw).unwrap();
    assert!(matches!(
        bind_source_view(
            &conn,
            &DataSourceId("source".into()),
            factory.as_ref(),
            temp.path(),
            false
        ),
        Err(ResumeIdentityError::Unbound)
    ));
}
