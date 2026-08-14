use super::*;

#[test]
fn reader_open_failure_aborts_the_mft_scan() {
    let temporary = tempfile::tempdir().unwrap();
    let config = ScanConfig {
        e01_path: temporary.path().join("missing.E01"),
        volume_offset: 0,
        mft_cluster: 0,
        cluster_size: 4096,
        bytes_per_sector: 512,
        mft_data_size: 1024,
        total_records: 1,
        scanner_record_size: 1024,
        data_runs: Vec::new(),
        partition_index: Some(0),
    };
    let connection = Connection::open_in_memory().unwrap();
    let result = run_scan(
        &connection,
        &DataSourceId("source".to_string()),
        &config,
        None,
        None,
    );

    let error = match result {
        Ok(_) => panic!("a missing evidence image must not produce a partial successful scan"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("MFT reader failure"));
    assert!(error
        .to_string()
        .contains("failed to open the evidence image"));
}

#[test]
fn second_batch_persistence_failure_rolls_back_the_first_batch() {
    let connection = Connection::open_in_memory().unwrap();
    persistence_sqlite::migrations::runner::run_source_all(&connection).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_second_mft_batch
             BEFORE INSERT ON file_entries
             WHEN NEW.id = 'mft:2000'
             BEGIN
                 SELECT RAISE(ABORT, 'synthetic second batch failure');
             END;",
        )
        .unwrap();
    let transaction = connection.unchecked_transaction().unwrap();
    let (sender, receiver) = bounded(2);
    sender.send(test_entries(0, MFT_DB_BATCH_SIZE)).unwrap();
    sender.send(test_entries(MFT_DB_BATCH_SIZE, 1)).unwrap();
    drop(sender);

    let processed = AtomicU64::new((MFT_DB_BATCH_SIZE + 1) as u64);
    let pipeline_stop = AtomicBool::new(false);
    let error = match collect_entries(
        &transaction,
        receiver,
        &processed,
        &pipeline_stop,
        (MFT_DB_BATCH_SIZE + 1) as u64,
        None,
    ) {
        Ok(_) => panic!("the second batch must surface its original SQLite error"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("synthetic second batch failure"));
    assert!(pipeline_stop.load(Ordering::Relaxed));
    let rows_inside_transaction: i64 = transaction
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows_inside_transaction, MFT_DB_BATCH_SIZE as i64);
    transaction.rollback().unwrap();
    let rows_after_rollback: i64 = connection
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows_after_rollback, 0);
}

fn test_entries(start: usize, count: usize) -> Vec<FileEntry> {
    (start..start + count)
        .map(|record_number| FileEntry {
            id: domain::FileEntryId(format!("mft:{record_number}")),
            parent_id: None,
            data_source_id: DataSourceId("source".to_string()),
            path: String::new(),
            name: format!("file-{record_number}"),
            entry_type: EntryType::File,
            size: Some(1),
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
        })
        .collect()
}
