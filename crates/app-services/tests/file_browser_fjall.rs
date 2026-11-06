use app_services::{
    case_service, file_service, source_db, storage_runtime::ReadWriteSourceStorageHandle,
};
use chrono::Utc;
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, FileEntryId};
use persistence_fjall::{CatalogEntryType, FileEntryRecord, PersistMode};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use transport::commands::GetFileRowsRequest;

#[path = "support/fjall_projection.rs"]
mod projection_support;

fn record(
    id: &str,
    parent_id: Option<&str>,
    name: &str,
    entry_type: CatalogEntryType,
    hidden: bool,
    system: bool,
    deleted: bool,
) -> FileEntryRecord {
    FileEntryRecord {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        data_source_id: "browser-source".to_owned(),
        path: name.to_owned(),
        name: name.to_owned(),
        entry_type,
        size: Some(1),
        ext: Some("txt".to_owned()),
        deleted,
        hidden,
        system,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: Some(Utc::now()),
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: None,
        unix_mode: None,
    }
}

fn scoped(local: &str) -> String {
    source_db::GlobalFileId::new(
        DataSourceId("browser-source".to_owned()),
        FileEntryId(local.to_owned()),
    )
    .encode()
    .0
}

fn write_catalog(source: &ReadWriteSourceStorageHandle) {
    for record in [
        record(
            "root",
            None,
            "root",
            CatalogEntryType::Directory,
            false,
            false,
            false,
        ),
        record(
            "orphan",
            Some("missing-parent"),
            "orphan",
            CatalogEntryType::Directory,
            false,
            false,
            false,
        ),
        record(
            "dir",
            Some("root"),
            "dir2",
            CatalogEntryType::Directory,
            false,
            false,
            false,
        ),
        record(
            "normal",
            Some("root"),
            "file2",
            CatalogEntryType::File,
            false,
            false,
            false,
        ),
        record(
            "hidden",
            Some("root"),
            "hidden",
            CatalogEntryType::File,
            true,
            false,
            false,
        ),
        record(
            "system",
            Some("root"),
            "system",
            CatalogEntryType::File,
            false,
            true,
            false,
        ),
        record(
            "deleted",
            Some("root"),
            "deleted",
            CatalogEntryType::File,
            false,
            false,
            true,
        ),
    ] {
        source.catalog_upsert(&record, PersistMode::Buffer).unwrap();
    }
    source.persist(PersistMode::SyncAll).unwrap();
}

#[test]
fn file_browser_fjall_matches_visibility_sorting_and_orphan_root_rules() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "Fjall browser", None).unwrap();
    let source = DataSource {
        id: DataSourceId("browser-source".to_owned()),
        name: "Browser source".to_owned(),
        kind: DataSourceKind::E01,
        source_path: temporary.path().join("browser-source.E01"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    case.with_conn(|connection| {
        let mut storage = DataSourceStorage::source_db(&source.id.0, Some("linux"), None);
        storage.import_state = "ready".to_owned();
        DataSourceRepo::new(connection).insert_with_storage(&case.meta.id, &source, &storage)
    })
    .unwrap();
    let source_conn = source_db::open_source_db(&case.case_root, &source.id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&case.meta.id, &source)
        .unwrap();
    drop(source_conn);

    let fjall =
        source_db::open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    write_catalog(&fjall);
    drop(fjall);

    let incomplete_tree = case
        .with_conn(|connection| {
            file_service::get_file_tree_for_case(connection, &case.case_root, &case.meta.id, false)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert!(incomplete_tree.is_empty());

    let fjall =
        source_db::open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    projection_support::mark_source_projection_complete(&fjall, &source.id.0);
    drop(fjall);

    let tree = case
        .with_conn(|connection| {
            file_service::get_file_tree_for_case(connection, &case.case_root, &case.meta.id, false)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert!(tree.iter().any(|node| node.id == scoped("orphan")));

    let rows = case
        .with_conn(|connection| {
            file_service::get_file_rows_for_case(
                connection,
                &case.case_root,
                &case.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(scoped("root")),
                    limit: 20,
                    ..Default::default()
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert_eq!(rows.total_count, 3);
    assert_eq!(rows.rows[0].id, scoped("dir"));
    assert_eq!(rows.rows[1].id, scoped("normal"));
    assert_eq!(rows.rows[2].id, scoped("deleted"));
}
