use app_services::{case_service, file_service};
use evidence_core::LogicalFsReader;
use persistence_source::fjall::{PersistMode, ScanDirection};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use tempfile::TempDir;
use transport::{commands::GetFileRowsRequest, dto::ViewerRangeRequestDto};

fn publish_source(source_conn: &rusqlite::Connection, source_id: &domain::DataSourceId) {
    let store = persistence_source::detail::open_store_for_write(source_conn)
        .unwrap()
        .unwrap();
    persistence_source::fjall::seal_source_generation(
        &store,
        &source_id.0,
        "fixture",
        &"a".repeat(64),
    )
    .unwrap();
    let key = persistence_source::fjall::encode_key(
        "projection",
        &[persistence_source::fjall::KeyPart::Text(
            source_id.0.clone(),
        )],
    )
    .unwrap();
    let marker = persistence_source::fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source_id.0),
        &source_id.0,
        1,
        source_id.0.as_bytes().to_vec(),
    );
    store
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            PersistMode::SyncAll,
        )
        .unwrap();
}

fn register_source(
    active: &app_services::active_case::ActiveCase,
    source_id: &str,
    source_name: &str,
    evidence_dir: &std::path::Path,
) -> domain::DataSourceId {
    let id = domain::DataSourceId(source_id.to_string());
    let source = domain::DataSource {
        id: id.clone(),
        name: source_name.to_string(),
        kind: domain::DataSourceKind::LogicalDirectory,
        source_path: evidence_dir.to_path_buf(),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    active
        .with_conn(|conn| {
            let mut storage = DataSourceStorage::source_db(&id.0, Some("linux"), None);
            storage.import_state = "ready".to_string();
            DataSourceRepo::new(conn).insert_with_storage(&active.meta.id, &source, &storage)
        })
        .unwrap();
    let source_conn = app_services::source_db::open_source_db(&active.case_root, &id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&active.meta.id, &source)
        .unwrap();
    let fs = LogicalFsReader::open(evidence_dir, source_name)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        .unwrap();
    file_service::enumerate_filesystem(&source_conn, &id, &fs).unwrap();
    publish_source(&source_conn, &id);
    id
}

fn import_fixture_directory(tmp: &TempDir) -> app_services::active_case::ActiveCase {
    let evidence_dir = tmp.path().join("evidence");
    std::fs::create_dir_all(evidence_dir.join("subdir")).unwrap();
    std::fs::create_dir_all(evidence_dir.join("emptydir")).unwrap();
    std::fs::write(evidence_dir.join("root.txt"), b"0123456789abcdef").unwrap();
    std::fs::write(
        evidence_dir.join("subdir").join("nested.bin"),
        [0xDE, 0xAD, 0xBE, 0xEF],
    )
    .unwrap();

    let active =
        case_service::create_case(&tmp.path().join("cases"), "files", Some("tester")).unwrap();
    register_source(&active, "ds-logical", "fixture", &evidence_dir);

    active
}

fn import_visibility_fixture_directory(tmp: &TempDir) -> app_services::active_case::ActiveCase {
    let evidence_dir = tmp.path().join("visibility-evidence");
    std::fs::create_dir_all(evidence_dir.join("visible-dir")).unwrap();
    std::fs::create_dir_all(evidence_dir.join("System Volume Information")).unwrap();
    std::fs::create_dir_all(evidence_dir.join("$Recycle.Bin")).unwrap();
    std::fs::write(evidence_dir.join("visible.txt"), b"visible").unwrap();
    std::fs::write(evidence_dir.join(".secret.txt"), b"hidden").unwrap();
    std::fs::write(evidence_dir.join("pagefile.sys"), b"system").unwrap();

    let active =
        case_service::create_case(&tmp.path().join("cases"), "visibility", Some("tester")).unwrap();
    register_source(
        &active,
        "ds-visibility",
        "visibility-fixture",
        &evidence_dir,
    );

    active
}

#[test]
fn directory_tree_and_children_return_only_directories() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let tree = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(tree.len(), 1);
            assert_eq!(tree[0].depth, 0);

            let children_result = file_service::get_file_children_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &tree[0].id,
                0,
                500,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let child_names: Vec<&str> = children_result
                .children
                .iter()
                .map(|node| node.name.as_str())
                .collect();
            assert_eq!(child_names, vec!["emptydir", "subdir"]);
            assert!(children_result.children.iter().all(|node| node.depth == 1));

            let rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(tree[0].id.clone()),
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let row_names: Vec<&str> = rows.rows.iter().map(|row| row.name.as_str()).collect();
            assert_eq!(row_names, vec!["emptydir", "subdir", "root.txt"]);

            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| row.id.clone())
                .unwrap();
            let no_directory_children = file_service::get_file_children_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &file_id,
                0,
                500,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(no_directory_children.children.is_empty());

            Ok(())
        })
        .unwrap();
}

#[test]
fn file_tree_real_contains_nested_directories_for_navigation() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let tree = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let names: Vec<&str> = tree.iter().map(|node| node.name.as_str()).collect();
            assert_eq!(names, vec!["evidence"]);
            assert_eq!(tree[0].depth, 0);
            assert_eq!(tree[0].expanded, Some(true));

            let children_result = file_service::get_file_children_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &tree[0].id,
                0,
                500,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let child_names: Vec<&str> = children_result
                .children
                .iter()
                .map(|node| node.name.as_str())
                .collect();
            assert_eq!(child_names, vec!["emptydir", "subdir"]);
            assert!(children_result.children.iter().all(|node| node.depth == 1));
            assert!(children_result
                .children
                .iter()
                .all(|node| node.expanded == Some(false)));

            Ok(())
        })
        .unwrap();
}

#[test]
fn file_rows_request_returns_direct_children_for_parent() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let root = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
            .pop()
            .unwrap();
            let root_rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let subdir = root_rows
                .rows
                .iter()
                .find(|row| row.name == "subdir")
                .map(|row| row.id.clone())
                .unwrap();

            let subdir_rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(subdir),
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            assert_eq!(subdir_rows.rows.len(), 1);
            assert_eq!(subdir_rows.rows[0].name, "nested.bin");
            assert_eq!(subdir_rows.rows[0].entry_type, "file");

            Ok(())
        })
        .unwrap();
}

#[test]
fn file_rows_and_children_are_limited_for_lazy_loading() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let root = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
            .pop()
            .unwrap();

            let first_dir = file_service::get_file_children_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &root.id,
                0,
                1,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(first_dir.children.len(), 1);
            assert_eq!(first_dir.total_count, 2);
            assert_eq!(first_dir.truncated, Some(true));

            let first_rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    offset: 0,
                    limit: 2,
                    show_hidden: false,
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(first_rows.rows.len(), 2);
            assert_eq!(first_rows.total_count, 3);
            assert!(first_rows.truncated);

            Ok(())
        })
        .unwrap();
}

#[test]
fn file_browser_visibility_respects_show_hidden_for_rows_and_tree() {
    let tmp = TempDir::new().unwrap();
    let active = import_visibility_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let root = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
            .pop()
            .unwrap();

            let visible_tree = file_service::get_file_children_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &root.id,
                0,
                50,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let visible_tree_names: Vec<&str> = visible_tree
                .children
                .iter()
                .map(|node| node.name.as_str())
                .collect();
            assert_eq!(visible_tree_names, vec!["visible-dir"]);

            let visible_rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(root.id.clone()),
                    offset: 0,
                    limit: 50,
                    show_hidden: false,
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let visible_row_names: Vec<&str> = visible_rows
                .rows
                .iter()
                .map(|row| row.name.as_str())
                .collect();
            assert_eq!(visible_row_names, vec!["visible-dir", "visible.txt"]);
            assert!(!visible_rows.rows.iter().any(|row| row.hidden || row.system));

            let all_tree = file_service::get_file_children_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &root.id,
                0,
                50,
                true,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let svi = all_tree
                .children
                .iter()
                .find(|node| node.name == "System Volume Information")
                .expect("show_hidden=true should surface System Volume Information");
            assert!(svi.hidden);
            assert!(svi.system);

            let recycle_bin = all_tree
                .children
                .iter()
                .find(|node| node.name == "$Recycle.Bin")
                .expect("show_hidden=true should surface $Recycle.Bin");
            assert!(recycle_bin.hidden);
            assert!(recycle_bin.system);

            let all_rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    offset: 0,
                    limit: 50,
                    show_hidden: true,
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(all_rows.total_count > visible_rows.total_count);

            let hidden_file = all_rows
                .rows
                .iter()
                .find(|row| row.name == ".secret.txt")
                .expect("show_hidden=true should surface dot-hidden files");
            assert!(hidden_file.hidden);
            assert!(!hidden_file.system);

            let system_file = all_rows
                .rows
                .iter()
                .find(|row| row.name == "pagefile.sys")
                .expect("show_hidden=true should surface inferred system files");
            assert!(system_file.hidden);
            assert!(system_file.system);

            Ok(())
        })
        .unwrap();
}

#[test]
fn deterministic_handle_reads_real_logical_file_bytes_as_hex() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let root = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
            .pop()
            .unwrap();
            let rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| row.id.clone())
                .unwrap();

            let handle = file_service::open_file_handle_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &file_id,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(handle.handle_id, format!("file:{file_id}"));
            assert_eq!(handle.size, 16);
            assert_eq!(handle.mime.as_deref(), Some("text/plain"));

            let range = file_service::read_file_range_for_source_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 1,
                    length: 5,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            assert_eq!(range.kind, "hex");
            assert!(range.lines.is_empty());
            assert_eq!(range.raw_bytes.unwrap(), b"12345");

            Ok(())
        })
        .unwrap();
}

#[test]
fn read_file_range_rejects_invalid_handles_instead_of_faking_bytes() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let err = file_service::read_file_range_for_case(
                conn,
                &ViewerRangeRequestDto {
                    handle_id: "handle-random".to_string(),
                    offset: 0,
                    length: 16,
                },
            )
            .unwrap_err();
            assert!(err.to_string().contains("Invalid file handle"));
            Ok(())
        })
        .unwrap();
}

#[test]
fn open_file_content_by_id_reads_uuid_backed_logical_file() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let root = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
            .pop()
            .unwrap();
            let rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| domain::FileEntryId(row.id.clone()))
                .unwrap();

            let handle = file_service::open_file_handle_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &file_id.0,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let range = file_service::read_file_range_for_source_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 0,
                    length: 4,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(range.raw_bytes.unwrap(), b"0123");

            Ok(())
        })
        .unwrap();
}

#[test]
fn open_file_content_rejects_traversal_paths_from_database() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let root = file_service::get_file_tree_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
            .pop()
            .unwrap();
            let rows = file_service::get_file_rows_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| row.id.clone())
                .unwrap();

            let source_id = domain::DataSourceId("ds-logical".to_string());
            let source_conn = app_services::source_db::open_registered_source_db(
                conn,
                &active.case_root,
                &source_id,
            )?;
            let global_file_id = app_services::source_db::GlobalFileId::parse(
                &domain::FileEntryId(file_id.clone()),
            )?;
            persistence_source::detail::update_catalog_path(
                &source_conn,
                &global_file_id.local_id.0,
                "../outside.txt",
                &global_file_id.data_source_id.0,
            )?;

            let handle = file_service::open_file_handle_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &file_id,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let err = file_service::read_file_range_for_source_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 0,
                    length: 16,
                },
            )
            .unwrap_err();
            assert!(err.to_string().contains("Unsafe file path"));

            Ok(())
        })
        .unwrap();
}

#[test]
fn encrypted_file_export_fails_before_creating_destination() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);
    let destination = tmp.path().join("exported.bin");

    active
        .with_conn(|conn| {
            let source_id = domain::DataSourceId("ds-logical".to_string());
            let source_conn = app_services::source_db::open_registered_source_db(
                conn,
                &active.case_root,
                &source_id,
            )?;
            let store = persistence_source::detail::open_store_for_write(&source_conn)?
                .ok_or_else(|| {
                    persistence_sqlite::DbError::System("source storage is missing".to_string())
                })?;
            let catalog =
                persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone());
            let snapshot = catalog.read_snapshot();
            let mut record = catalog
                .list_all(&snapshot, &source_id.0, None, ScanDirection::Forward, 512)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?
                .records
                .into_iter()
                .find(|record| record.name == "root.txt")
                .ok_or_else(|| {
                    persistence_sqlite::DbError::System(
                        "root.txt catalog record is missing".to_string(),
                    )
                })?;
            let file_id = record.id.clone();
            record.encrypted = Some(true);
            catalog
                .upsert_batch(&[record], PersistMode::SyncAll)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let global_file_id = app_services::source_db::GlobalFileId::new(
                source_id.clone(),
                domain::FileEntryId(file_id),
            )
            .encode()
            .0;

            let error = file_service::extract_file_to_destination_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
                &global_file_id,
                &destination,
                false,
            )
            .unwrap_err();
            assert!(matches!(
                error,
                file_service::FileServiceError::Unsupported(_)
            ));
            assert!(!destination.exists());
            Ok(())
        })
        .unwrap();
}
