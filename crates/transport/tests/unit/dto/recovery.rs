use super::*;

#[test]
fn test_deleted_file_recovery_dto_serializes_camel_case() {
    let dto = DeletedFileRecoveryDto {
        original_path: "$OrphanInode77/journal_recovered_inode_77".to_string(),
        inode: "77".to_string(),
        declared_size: 4096,
        block_count: 1,
        recovery_method: "journal_descriptor".to_string(),
        confidence: 0.85,
        filesystem_type: "ext4".to_string(),
        recovered_bytes: 1024,
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(
        value["originalPath"],
        "$OrphanInode77/journal_recovered_inode_77"
    );
    assert_eq!(value["inode"], "77");
    assert_eq!(value["declaredSize"], 4096);
    assert_eq!(value["blockCount"], 1);
    assert_eq!(value["recoveryMethod"], "journal_descriptor");
    assert_eq!(value["confidence"], 0.85);
    assert_eq!(value["filesystemType"], "ext4");
    assert_eq!(value["recoveredBytes"], 1024);
    // Verify camelCase keys
    assert!(value.get("original_path").is_none());
    assert!(value.get("declared_size").is_none());
    assert!(value.get("block_count").is_none());
    assert!(value.get("recovery_method").is_none());
    assert!(value.get("filesystem_type").is_none());
    assert!(value.get("recovered_bytes").is_none());
}

#[test]
fn test_deleted_file_recovery_dto_deserializes_camel_case() {
    let json = r#"{
            "originalPath": "$OrphanInode42/log_recovered_inode_42",
            "inode": "42",
            "declaredSize": 2048,
            "blockCount": 2,
            "recoveryMethod": "xlog_inode_item_format_2",
            "confidence": 0.60,
            "filesystemType": "xfs",
            "recoveredBytes": 8192
        }"#;

    let dto: DeletedFileRecoveryDto = serde_json::from_str(json).unwrap();

    assert_eq!(dto.original_path, "$OrphanInode42/log_recovered_inode_42");
    assert_eq!(dto.inode, "42");
    assert_eq!(dto.declared_size, 2048);
    assert_eq!(dto.block_count, 2);
    assert_eq!(dto.recovery_method, "xlog_inode_item_format_2");
    assert_eq!(dto.confidence, 0.60);
    assert_eq!(dto.filesystem_type, "xfs");
    assert_eq!(dto.recovered_bytes, 8192);
}

#[test]
fn test_deleted_file_recovery_dto_optional_fields_skip_when_none() {
    let dto = DeletedFileRecoveryDto {
        original_path: "/lost+found/file.txt".to_string(),
        inode: "5".to_string(),
        declared_size: 0,
        block_count: 0,
        recovery_method: "dirent_hint".to_string(),
        confidence: 0.0,
        filesystem_type: "ext4".to_string(),
        recovered_bytes: 0,
    };

    let value = serde_json::to_value(&dto).unwrap();
    // All fields should be present since they are not Optional
    assert_eq!(value["originalPath"], "/lost+found/file.txt");
    assert_eq!(value["confidence"], 0.0);
    assert_eq!(value["recoveredBytes"], 0);
}
