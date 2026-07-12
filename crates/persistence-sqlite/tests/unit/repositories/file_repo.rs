use super::*;
use crate::{open_or_create, runner};
use domain::{DataSourceKind, EntryType};
use tempfile::TempDir;

fn insert_data_source(conn: &Connection, id: &DataSourceId) {
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('case-1', 'Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES (?1, 'case-1', 'ds', ?2, 'C:/evidence', '2026-01-01T00:00:00Z')",
        params![id.0, DataSourceKind::LogicalDirectory.to_string()],
    )
    .unwrap();
}

fn entry(id: &str, ds_id: &DataSourceId, path: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: ds_id.clone(),
        path: path.to_string(),
        name: path.to_string(),
        entry_type: EntryType::File,
        size: Some(1),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

#[test]
fn find_by_path_prefix_escapes_like_wildcards() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-like".to_string());
    insert_data_source(&conn, &ds_id);
    let repo = FileRepo::new(&conn);
    repo.insert_batch(&[
        entry("literal", &ds_id, "root/test%file/a.txt"),
        entry("wildcard", &ds_id, "root/testXfile/a.txt"),
        entry("underscore-literal", &ds_id, "root/test_file/a.txt"),
        entry("underscore-wildcard", &ds_id, "root/testZfile/a.txt"),
    ])
    .unwrap();

    let percent = repo.find_by_path_prefix(&ds_id, "root/test%file").unwrap();
    assert_eq!(percent.len(), 1);
    assert_eq!(percent[0].id.0, "literal");

    let underscore = repo.find_by_path_prefix(&ds_id, "root/test_file").unwrap();
    assert_eq!(underscore.len(), 1);
    assert_eq!(underscore[0].id.0, "underscore-literal");
}

#[test]
fn legacy_capitalized_entry_type_is_treated_as_directory() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-legacy-entry-type".to_string());
    insert_data_source(&conn, &ds_id);
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES ('root-dir', NULL, ?1, 'EFI', 'EFI', 'Directory', 0)",
        params![ds_id.0],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES ('child-dir', 'root-dir', ?1, 'EFI/Boot', 'Boot', 'Directory', 0)",
        params![ds_id.0],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES ('child-file', 'root-dir', ?1, 'EFI/bootx64.efi', 'bootx64.efi', 'File', 4096)",
        params![ds_id.0],
    )
    .unwrap();

    let repo = FileRepo::new(&conn);
    let root_id = FileEntryId("root-dir".to_string());
    let roots = repo.find_root_directories().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].entry_type, EntryType::Directory);
    assert_eq!(roots[0].id.0, root_id.0);

    assert!(repo.has_child_directories(&root_id).unwrap());
    let child_dirs = repo.find_child_directories(&root_id).unwrap();
    assert_eq!(child_dirs.len(), 1);
    assert_eq!(child_dirs[0].entry_type, EntryType::Directory);

    let counts = repo.count_child_directories_batch(&[&root_id]).unwrap();
    assert_eq!(counts.get("root-dir"), Some(&1));
}
