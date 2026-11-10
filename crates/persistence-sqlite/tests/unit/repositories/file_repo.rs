use crate::repositories::file_repo::FileRepo;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};

#[test]
fn file_repository_writes_catalog_records_to_fjall_source_storage() {
    let root = tempfile::tempdir().unwrap();
    let connection = crate::open_or_create_source(&root.path().join("source.db")).unwrap();
    crate::source_fjall::configure_source_identity(&connection, "source-1").unwrap();
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
