use super::*;

#[test]
fn rollback_surfaces_tombstone_cleanup_failure_after_restoring_paths() {
    let temp = tempfile::TempDir::new().unwrap();
    let original = temp.path().join("source");
    let tombstone_root = temp.path().join("tombstone");
    let staged_path = tombstone_root.join("source");
    std::fs::create_dir_all(&staged_path).unwrap();
    std::fs::write(staged_path.join("marker"), b"restorable").unwrap();
    std::fs::write(tombstone_root.join("cleanup-blocker"), b"block remove_dir").unwrap();
    let staged = [StagedDataSourcePath {
        label: "source",
        original: original.clone(),
        tombstone: staged_path,
    }];

    let error = rollback_staged_deletion(
        "ds-rollback-cleanup",
        &tombstone_root,
        "cache/data-source-tombstones/ds-rollback-cleanup",
        &staged,
        "database transaction failed".to_string(),
        CaseServiceError::InvalidCaseDir("primary failure".to_string()),
    );

    match error {
        CaseServiceError::DataSourceDeleteRollbackFailed {
            data_source_id,
            tombstone,
            step,
            original,
            rollback,
        } => {
            assert_eq!(data_source_id, "ds-rollback-cleanup");
            assert_eq!(
                tombstone,
                "cache/data-source-tombstones/ds-rollback-cleanup"
            );
            assert_eq!(step, "cleanupEmptyTombstone");
            assert!(matches!(
                *original,
                CaseServiceError::InvalidCaseDir(ref message) if message == "primary failure"
            ));
            assert_eq!(rollback.kind(), std::io::ErrorKind::DirectoryNotEmpty);
            assert!(rollback.to_string().contains("database transaction failed"));
            assert!(rollback.to_string().contains("cleanup failed"));
        }
        other => panic!("unexpected rollback error: {other:?}"),
    }
    assert_eq!(
        std::fs::read(original.join("marker")).unwrap(),
        b"restorable"
    );
    assert!(tombstone_root.join("cleanup-blocker").is_file());
}
