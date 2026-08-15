use super::*;
use crate::{open_in_memory, open_or_create, runner};
use domain::{
    DataSourceId, DataSourceKind, EntryType, FileEncryptionStatus, FileEntry, FileEntryId,
};
use rusqlite::{params, Connection};
use tempfile::TempDir;

use crate::repositories::catalog_file_repo::CatalogFileRepo;

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

fn catalog_entry(
    id: &str,
    parent_id: Option<&str>,
    ds_id: &DataSourceId,
    path: &str,
    name: &str,
    entry_type: EntryType,
) -> FileEntry {
    let mut value = entry(id, ds_id, path);
    value.parent_id = parent_id.map(|parent| FileEntryId(parent.to_string()));
    value.name = name.to_string();
    value.entry_type = entry_type.clone();
    value.size = (entry_type == EntryType::File).then_some(1);
    value
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
fn encrypted_flag_round_trips_through_file_repository() {
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-encrypted".to_string());
    let mut encrypted = entry("efs-file", &ds_id, "Users/alice/secret.txt");
    encrypted.encrypted = true;

    let repo = FileRepo::new(&conn);
    repo.insert_batch(&[encrypted]).unwrap();

    let stored = repo
        .find_by_id(&FileEntryId("efs-file".to_string()))
        .unwrap()
        .expect("encrypted file entry");
    assert!(stored.encrypted);
    assert_eq!(
        repo.find_encryption_status(&FileEntryId("efs-file".to_string()))
            .unwrap(),
        Some(FileEncryptionStatus::Encrypted)
    );

    repo.insert_batch(&[entry("clear-file", &ds_id, "Users/alice/plain.txt")])
        .unwrap();
    assert_eq!(
        repo.find_encryption_status(&FileEntryId("clear-file".to_string()))
            .unwrap(),
        Some(FileEncryptionStatus::Clear)
    );
}

#[test]
fn unknown_encryption_status_is_not_projected_as_clear() {
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type)
         VALUES ('legacy-file', 'legacy-source', 'old.bin', 'old.bin', 'file')",
        [],
    )
    .unwrap();
    let repo = FileRepo::new(&conn);

    assert_eq!(
        repo.find_encryption_status(&FileEntryId("legacy-file".to_string()))
            .unwrap(),
        Some(FileEncryptionStatus::Unknown)
    );
    let entry = repo
        .find_by_id(&FileEntryId("legacy-file".to_string()))
        .unwrap()
        .unwrap();
    assert!(entry.encrypted, "legacy bool projection must fail closed");
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

#[test]
fn reads_and_assigns_partition_index_by_file_id() {
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-partition-index".to_string());
    let repo = FileRepo::new(&conn);
    repo.insert_batch(&[
        FileEntry {
            id: FileEntryId("root".to_string()),
            parent_id: None,
            data_source_id: ds_id.clone(),
            path: String::new(),
            name: "Partition 9 (XFS)".to_string(),
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
            id: FileEntryId("child".to_string()),
            parent_id: Some(FileEntryId("root".to_string())),
            data_source_id: ds_id,
            path: "etc".to_string(),
            name: "etc".to_string(),
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
    ])
    .unwrap();

    assert_eq!(
        repo.find_partition_index_by_id(&FileEntryId("child".to_string()))
            .unwrap(),
        None
    );
    assert_eq!(
        repo.assign_partition_index_to_subtree(&FileEntryId("root".to_string()), 9)
            .unwrap(),
        2
    );
    assert_eq!(
        repo.find_partition_index_by_id(&FileEntryId("child".to_string()))
            .unwrap(),
        Some(9)
    );
    assert_eq!(
        repo.find_partition_index_by_id(&FileEntryId("missing".to_string()))
            .unwrap(),
        None
    );
}

#[test]
fn mount_catalog_queries_are_partition_scoped_and_accept_prefixed_paths() {
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-mount".to_string());
    let transaction = conn.unchecked_transaction().unwrap();
    let mut deleted = catalog_entry(
        "deleted",
        Some("root-2"),
        &ds_id,
        "[P2]/deleted",
        "deleted",
        EntryType::File,
    );
    deleted.deleted = true;
    CatalogFileRepo::new(&transaction)
        .insert_batch_with_partition_index_in_transaction(
            &[
                catalog_entry(
                    "root-2",
                    None,
                    &ds_id,
                    "",
                    "Partition 2 (NTFS)",
                    EntryType::Directory,
                ),
                catalog_entry(
                    "etc",
                    Some("root-2"),
                    &ds_id,
                    "[P2]/etc",
                    "etc",
                    EntryType::Directory,
                ),
                catalog_entry(
                    "hosts",
                    Some("root-2"),
                    &ds_id,
                    "[P2]/etc/hosts",
                    "hosts",
                    EntryType::File,
                ),
                deleted,
            ],
            2,
        )
        .unwrap();
    CatalogFileRepo::new(&transaction)
        .insert_batch_with_partition_index_in_transaction(
            &[catalog_entry(
                "root-1",
                None,
                &ds_id,
                "",
                "Partition 1 (FAT32)",
                EntryType::Directory,
            )],
            1,
        )
        .unwrap();
    transaction.commit().unwrap();

    let repo = FileRepo::new(&conn);
    let root = repo
        .find_root_for_partition(&ds_id, 2)
        .unwrap()
        .expect("partition root");
    assert_eq!(root.id.0, "root-2");
    let first_page = repo
        .find_children_page_for_partition(&root.id, &ds_id, 2, 0, 1)
        .unwrap();
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0].id.0, "etc");
    let second_page = repo
        .find_children_page_for_partition(&root.id, &ds_id, 2, 1, 1)
        .unwrap();
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].id.0, "hosts");
    assert!(repo
        .find_children_page_for_partition(&root.id, &ds_id, 2, 2, 1)
        .unwrap()
        .is_empty());
    let mount_children = repo
        .find_mount_children_for_partition(&root.id, &ds_id, 2)
        .unwrap();
    assert_eq!(
        mount_children
            .iter()
            .map(|entry| entry.id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["etc", "hosts"]
    );
    assert_eq!(
        repo.find_by_partition_and_path(&ds_id, 2, "etc/hosts")
            .unwrap()
            .expect("partition-prefixed path")
            .id
            .0,
        "hosts"
    );
    assert!(repo
        .find_by_partition_and_path(&ds_id, 1, "etc/hosts")
        .unwrap()
        .is_none());
}

#[test]
fn read_only_and_archive_flags_round_trip_through_the_repository() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-attrs".to_string());
    insert_data_source(&conn, &ds_id);
    let repo = FileRepo::new(&conn);

    let mut flagged = entry("flagged", &ds_id, "root/flagged.txt");
    flagged.read_only = true;
    flagged.archive = true;
    flagged.unix_mode = Some(0o100644);
    let plain = entry("plain", &ds_id, "root/plain.txt");
    repo.insert_batch(&[flagged, plain]).unwrap();

    let loaded = repo
        .find_by_id(&FileEntryId("flagged".to_string()))
        .unwrap()
        .expect("flagged entry");
    assert!(loaded.read_only);
    assert!(loaded.archive);
    assert_eq!(loaded.unix_mode, Some(0o100644));
    assert!(!loaded.hidden);
    assert!(!loaded.system);

    let plain = repo
        .find_by_id(&FileEntryId("plain".to_string()))
        .unwrap()
        .expect("plain entry");
    assert!(!plain.read_only);
    assert!(!plain.archive);
    assert_eq!(plain.unix_mode, None);
}

#[test]
fn catalog_repo_insert_and_root_update_persist_attribute_flags() {
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-catalog-attrs".to_string());
    let repo = FileRepo::new(&conn);
    let catalog = CatalogFileRepo::new(&conn);

    let mut root = catalog_entry("root", None, &ds_id, "", "root", EntryType::Directory);
    root.read_only = true;
    root.archive = true;

    let mut child = catalog_entry(
        "child",
        Some("root"),
        &ds_id,
        "child.txt",
        "child.txt",
        EntryType::File,
    );
    child.read_only = true;
    child.archive = true;
    child.unix_mode = Some(0o100600);
    catalog
        .insert_batch_with_partition_index_in_transaction(&[root, child], 0)
        .unwrap();

    let loaded = repo
        .find_by_id(&FileEntryId("child".to_string()))
        .unwrap()
        .expect("catalog child");
    assert!(loaded.read_only);
    assert!(loaded.archive);
    assert_eq!(loaded.unix_mode, Some(0o100600));
    let loaded_root = repo
        .find_by_id(&FileEntryId("root".to_string()))
        .unwrap()
        .expect("catalog root");
    assert!(loaded_root.read_only);
    assert!(loaded_root.archive);
}
