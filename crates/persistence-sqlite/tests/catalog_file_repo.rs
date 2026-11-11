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
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

#[test]
fn checkpointed_catalog_batch_persists_partition_index_with_parent_links() {
    let directory = tempfile::tempdir().unwrap();
    let connection = persistence_sqlite::open_or_create_source(&directory.path().join("source.db"))
        .expect("open database");
    persistence_sqlite::source_fjall::configure_source_identity(&connection, "source-1").unwrap();
    connection
        .execute(
            "INSERT INTO data_sources
                (id, case_id, name, kind, source_path, imported_at)
             VALUES ('source-1', 'case-1', 'source', 'e01', 'image.E01', datetime('now'))",
            [],
        )
        .expect("insert source metadata");
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

    let store = persistence_sqlite::source_fjall::open_store(&connection)
        .unwrap()
        .unwrap();
    let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut rows = catalog
        .list_all(
            &snapshot,
            "source-1",
            None,
            persistence_fjall::ScanDirection::Forward,
            10,
        )
        .unwrap()
        .records
        .into_iter()
        .map(|record| {
            (
                record.id,
                record.parent_id,
                i64::from(record.partition_index.unwrap()),
            )
        })
        .collect::<Vec<_>>();
    rows.sort();

    assert_eq!(
        rows,
        vec![
            ("file".to_string(), Some("root".to_string()), 7),
            ("root".to_string(), None, 7),
        ]
    );
    let encrypted = catalog
        .get(&snapshot, "source-1", "file")
        .unwrap()
        .unwrap()
        .encrypted;
    assert_eq!(encrypted, Some(true));
    let fingerprint_table: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'forensic_fingerprints'",
            [],
            |row| row.get(0),
        )
        .expect("check source fingerprint table");
    assert_eq!(fingerprint_table, 0);
    let hash = "b".repeat(64);
    persistence_sqlite::repositories::file_repo::FileRepo::new(&connection)
        .update_hash_sha256(&FileEntryId("file".into()), &hash)
        .unwrap();
    let updated = catalog
        .get(&catalog.read_snapshot(), "source-1", "file")
        .unwrap()
        .unwrap();
    assert_eq!(updated.hash_sha256, Some(hash));
    assert_eq!(
        persistence_sqlite::repositories::file_repo::FileRepo::new(&connection)
            .find_partition_index_by_id(&FileEntryId("file".into()))
            .unwrap(),
        Some(7)
    );
}
