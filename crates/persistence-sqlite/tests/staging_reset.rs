use persistence_sqlite::repositories::staging_repo::StagingRepo;

#[test]
fn partition_staging_reset_clears_rows_and_transient_metadata_only() {
    let temporary = tempfile::TempDir::new().unwrap();
    let connection = StagingRepo::open_partition_staging_conn(temporary.path(), "source-a", 5)
        .expect("open partition staging");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, partition_index)
             VALUES ('mft:5:5', 'source-a', '\\', '\\', 'directory', 5)",
            [],
        )
        .unwrap();
    for (key, value) in [
        ("status", "failed"),
        ("error", "old failure"),
        ("merged", "true"),
        ("mft_fallback_warning", "old warning"),
        ("source_identity", "keep-me"),
    ] {
        StagingRepo::set_staging_meta(&connection, key, value).unwrap();
    }

    StagingRepo::reset_partition_staging(&connection).unwrap();

    assert_eq!(StagingRepo::staging_db_row_count(&connection).unwrap(), 0);
    for key in ["status", "error", "merged", "mft_fallback_warning"] {
        assert_eq!(
            StagingRepo::get_staging_meta(&connection, key).unwrap(),
            None
        );
    }
    assert_eq!(
        StagingRepo::get_staging_meta(&connection, "source_identity").unwrap(),
        Some("keep-me".to_string())
    );
}
