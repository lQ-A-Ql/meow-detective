use app_services::{case_service, file_service};
use evidence_core::LogicalFsReader;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use tempfile::TempDir;
use transport::{commands::GetFileRowsRequest, dto::ViewerRangeRequestDto};

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
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId("ds-logical".to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "fixture".to_string(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let fs = LogicalFsReader::open(&evidence_dir, "fixture")
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            file_service::enumerate_filesystem(conn, &ds_id, &fs)?;
            Ok(())
        })
        .unwrap();

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
        case_service::create_case(&tmp.path().join("cases"), "visibility", Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId("ds-visibility".to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "visibility-fixture".to_string(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let fs = LogicalFsReader::open(&evidence_dir, "visibility-fixture")
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            file_service::enumerate_filesystem(conn, &ds_id, &fs)?;
            Ok(())
        })
        .unwrap();

    active
}

#[test]
fn directory_tree_and_children_return_only_directories() {
    let tmp = TempDir::new().unwrap();
    let active = import_fixture_directory(&tmp);

    active
        .with_conn(|conn| {
            let tree = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?;
            assert_eq!(tree.len(), 1);
            assert_eq!(tree[0].depth, 0);

            let children_result = file_service::get_file_children_lazy(conn, &tree[0].id, 0, 500)
                .map_err(persistence_sqlite::DbError::System)?;
            let child_names: Vec<&str> = children_result
                .children
                .iter()
                .map(|node| node.name.as_str())
                .collect();
            assert_eq!(child_names, vec!["emptydir", "subdir"]);
            assert!(children_result.children.iter().all(|node| node.depth == 1));

            let rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(tree[0].id.clone()),
                    ..Default::default()
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
            let row_names: Vec<&str> = rows.rows.iter().map(|row| row.name.as_str()).collect();
            assert_eq!(row_names, vec!["emptydir", "subdir", "root.txt"]);

            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| row.id.clone())
                .unwrap();
            let no_directory_children =
                file_service::get_file_children_lazy(conn, &file_id, 0, 500)
                    .map_err(persistence_sqlite::DbError::System)?;
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
            let tree = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?;

            let names: Vec<&str> = tree.iter().map(|node| node.name.as_str()).collect();
            assert_eq!(names, vec!["evidence"]);
            assert_eq!(tree[0].depth, 0);
            assert_eq!(tree[0].expanded, Some(true));

            let children_result = file_service::get_file_children_lazy(conn, &tree[0].id, 0, 500)
                .map_err(persistence_sqlite::DbError::System)?;
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
            let root = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?
                .pop()
                .unwrap();
            let root_rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
            let subdir = root_rows
                .rows
                .iter()
                .find(|row| row.name == "subdir")
                .map(|row| row.id.clone())
                .unwrap();

            let subdir_rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(subdir),
                    ..Default::default()
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;

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
            let root = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?
                .pop()
                .unwrap();

            let first_dir = file_service::get_file_children_lazy(conn, &root.id, 0, 1)
                .map_err(persistence_sqlite::DbError::System)?;
            assert_eq!(first_dir.children.len(), 1);
            assert_eq!(first_dir.total_count, 2);
            assert_eq!(first_dir.truncated, Some(true));

            let first_rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    offset: 0,
                    limit: 2,
                    show_hidden: false,
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
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
            let root = file_service::get_file_tree_real_with_visibility(conn, false)
                .map_err(persistence_sqlite::DbError::System)?
                .pop()
                .unwrap();

            let visible_tree = file_service::get_file_children_lazy_with_visibility(
                conn, &root.id, 0, 50, false,
            )
            .map_err(persistence_sqlite::DbError::System)?;
            let visible_tree_names: Vec<&str> = visible_tree
                .children
                .iter()
                .map(|node| node.name.as_str())
                .collect();
            assert_eq!(visible_tree_names, vec!["visible-dir"]);

            let visible_rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(root.id.clone()),
                    offset: 0,
                    limit: 50,
                    show_hidden: false,
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
            let visible_row_names: Vec<&str> =
                visible_rows.rows.iter().map(|row| row.name.as_str()).collect();
            assert_eq!(visible_row_names, vec!["visible-dir", "visible.txt"]);
            assert!(!visible_rows.rows.iter().any(|row| row.hidden || row.system));

            let all_tree = file_service::get_file_children_lazy_with_visibility(
                conn, &root.id, 0, 50, true,
            )
            .map_err(persistence_sqlite::DbError::System)?;
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

            let all_rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    offset: 0,
                    limit: 50,
                    show_hidden: true,
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
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
            let root = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?
                .pop()
                .unwrap();
            let rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| row.id.clone())
                .unwrap();

            let handle = file_service::open_file_handle_real(conn, &file_id)
                .map_err(persistence_sqlite::DbError::System)?;
            assert_eq!(handle.handle_id, format!("file:{file_id}"));
            assert_eq!(handle.size, 16);
            assert_eq!(handle.mime.as_deref(), Some("text/plain"));

            let range = file_service::read_file_range_for_case(
                conn,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 1,
                    length: 5,
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;

            assert_eq!(range.kind, "hex");
            assert_eq!(range.lines, vec!["00000001  31 32 33 34 35"]);

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
            assert_eq!(err, "Invalid file handle");
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
            let root = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?
                .pop()
                .unwrap();
            let rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| domain::FileEntryId(row.id.clone()))
                .unwrap();

            let bytes = file_service::read_file_header_by_id(conn, &file_id, 4)
                .map_err(persistence_sqlite::DbError::System)?;
            assert_eq!(bytes, b"0123");

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
            let root = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?
                .pop()
                .unwrap();
            let rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some(root.id),
                    ..Default::default()
                },
            )
            .map_err(persistence_sqlite::DbError::System)?;
            let file_id = rows
                .rows
                .iter()
                .find(|row| row.name == "root.txt")
                .map(|row| row.id.clone())
                .unwrap();

            conn.execute(
                "UPDATE file_entries SET path = '../outside.txt' WHERE id = ?1",
                rusqlite::params![file_id],
            )?;

            let err = file_service::read_file_header_by_id(conn, &domain::FileEntryId(file_id), 16)
                .unwrap_err();
            assert!(err.contains("Unsafe file path"));

            Ok(())
        })
        .unwrap();
}
