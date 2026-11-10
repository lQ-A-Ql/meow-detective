use super::*;
crate::native_source_test_support!();
use persistence_sqlite::repositories::file_repo::FileRepo;
use test_support::NativeSourceFixture;

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
fn second_batch_failure_preserves_committed_first_batch_and_stops_pipeline() {
    let fixture = NativeSourceFixture::new("source", "case-mft");
    let (sender, receiver) = bounded(2);
    sender.send(test_entries(0, MFT_DB_BATCH_SIZE)).unwrap();
    let mut invalid = test_entries(MFT_DB_BATCH_SIZE, 1);
    invalid[0].id = domain::FileEntryId(String::new());
    sender.send(invalid).unwrap();
    drop(sender);

    let processed = AtomicU64::new((MFT_DB_BATCH_SIZE + 1) as u64);
    let pipeline_stop = AtomicBool::new(false);
    let error = match collect_entries(
        &fixture.connection,
        receiver,
        &processed,
        &pipeline_stop,
        (MFT_DB_BATCH_SIZE + 1) as u64,
        None,
    ) {
        Ok(_) => panic!("the second batch must surface its native validation error"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("identity fields are empty"));
    assert!(pipeline_stop.load(Ordering::Relaxed));
    assert_eq!(
        FileRepo::new(&fixture.connection)
            .count_by_data_source(&DataSourceId("source".into()))
            .unwrap(),
        MFT_DB_BATCH_SIZE as u64
    );
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
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        })
        .collect()
}
