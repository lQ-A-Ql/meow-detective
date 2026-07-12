use app_services::staging::{cleanup_staging, open_partition_staging, staging_dir};

#[test]
fn staging_cleanup_removes_checkpointed_directory() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-cleanup";
    {
        let _connection = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    }
    let dir = staging_dir(tmp.path(), ds_id);
    assert!(dir.exists());

    cleanup_staging(tmp.path(), ds_id);

    assert!(!dir.exists());
}
