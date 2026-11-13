use crate::repositories::file_repo::FileRepo;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};

fn file_entry(id: &str, path: &str, name: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.into()),
        parent_id: None,
        data_source_id: DataSourceId("source-1".into()),
        path: path.into(),
        name: name.into(),
        entry_type: EntryType::File,
        size: Some(1),
        ext: Some("txt".into()),
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
fn file_repository_finds_entries_by_id_batch() {
    let root = tempfile::tempdir().unwrap();
    let connection =
        crate::control::opening::open_source_database(&root.path().join("source.db"), "source-1")
            .unwrap();
    FileRepo::new(&connection)
        .insert_batch(&[
            file_entry("entry-1", "/etc/hosts", "hosts"),
            file_entry("entry-2", "/var/log/syslog", "syslog"),
        ])
        .unwrap();

    let resolved = FileRepo::new(&connection)
        .find_by_ids(&[
            FileEntryId("entry-1".into()),
            FileEntryId("missing".into()),
            FileEntryId("entry-2".into()),
            // Duplicate ids are deduplicated, not re-read.
            FileEntryId("entry-1".into()),
        ])
        .unwrap();

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved["entry-1"].path, "/etc/hosts");
    assert_eq!(resolved["entry-2"].name, "syslog");
    assert!(!resolved.contains_key("missing"));

    // Empty batches never touch storage.
    assert!(FileRepo::new(&connection)
        .find_by_ids(&[])
        .unwrap()
        .is_empty());
}

#[test]
fn file_repository_writes_catalog_records_to_fjall_source_storage() {
    let root = tempfile::tempdir().unwrap();
    let connection =
        crate::control::opening::open_source_database(&root.path().join("source.db"), "source-1")
            .unwrap();
    let entry = FileEntry {
        id: FileEntryId("entry-1".into()),
        parent_id: None,
        data_source_id: DataSourceId("source-1".into()),
        path: "entry.txt".into(),
        name: "entry.txt".into(),
        entry_type: EntryType::File,
        size: Some(1),
        ext: Some("txt".into()),
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
    };
    FileRepo::new(&connection)
        .insert_batch(std::slice::from_ref(&entry))
        .unwrap();
    assert_eq!(
        FileRepo::new(&connection)
            .find_by_id(&entry.id)
            .unwrap()
            .unwrap()
            .path,
        entry.path
    );
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
