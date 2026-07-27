use super::*;

#[test]
fn volume_status_uses_camel_case_without_secret_fields() {
    let status = BitLockerVolumeStatusDto {
        data_source_id: "source-1".to_string(),
        partition_index: 2,
        unlocked: false,
        encryption_method: "XTS-AES-256".to_string(),
        encryption_method_code: 0x8005,
        decryptable: true,
        bytes_per_sector: 512,
        metadata_fingerprint: "0123456789abcdef".to_string(),
        metadata_copy_count: 3,
        protectors: vec![BitLockerProtectorDto {
            code: 0x0800,
            kind: "recoveryPassword".to_string(),
            label: "recovery password".to_string(),
            unlockable: true,
        }],
        supports_password: false,
        supports_recovery_password: true,
        stored_key_available: false,
        plaintext_filesystem: None,
    };

    let value = serde_json::to_value(status).expect("status serializes");
    assert_eq!(value["dataSourceId"], "source-1");
    assert_eq!(value["partitionIndex"], 2);
    assert_eq!(value["metadataCopyCount"], 3);
    assert_eq!(value["storedKeyAvailable"], false);
    assert!(value.get("password").is_none());
    assert!(value.get("recoveryPassword").is_none());
    assert!(value.get("plaintextFilesystem").is_none());
}
