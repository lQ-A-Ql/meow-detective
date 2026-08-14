use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::catalog_file_repo::CatalogFileRepo;

fn entry(id: &str, parent_id: Option<&str>, path: &str, entry_type: EntryType) -> FileEntry {
    let is_file = entry_type == EntryType::File;
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: parent_id.map(|value| FileEntryId(value.to_string())),
        data_source_id: DataSourceId("source-1".to_string()),
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        entry_type,
        size: is_file.then_some(42),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

#[test]
fn checkpointed_catalog_batch_persists_partition_index_with_parent_links() {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let transaction = connection
        .unchecked_transaction()
        .expect("begin catalog batch");
    let mut encrypted_file = entry("file", Some("root"), "etc/passwd", EntryType::File);
    encrypted_file.encrypted = true;
    CatalogFileRepo::new(&transaction)
        .insert_batch_with_partition_index_in_transaction(
            &[
                entry("root", None, "", EntryType::Directory),
                encrypted_file,
            ],
            7,
        )
        .expect("insert catalog batch");
    transaction.commit().expect("commit catalog batch");

    let rows = connection
        .prepare(
            "SELECT id, parent_id, partition_index
             FROM file_entries
             ORDER BY id",
        )
        .expect("prepare query")
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .expect("query rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect rows");

    assert_eq!(
        rows,
        vec![
            ("file".to_string(), Some("root".to_string()), 7),
            ("root".to_string(), None, 7),
        ]
    );
    let encrypted: bool = connection
        .query_row(
            "SELECT encrypted <> 0 FROM file_entries WHERE id = 'file'",
            [],
            |row| row.get(0),
        )
        .expect("read encrypted flag");
    assert!(encrypted);
}
