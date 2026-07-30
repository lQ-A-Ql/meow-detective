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
        recovery_password_reconstruction: None,
    };

    let value = serde_json::to_value(status).expect("status serializes");
    assert_eq!(value["dataSourceId"], "source-1");
    assert_eq!(value["partitionIndex"], 2);
    assert_eq!(value["metadataCopyCount"], 3);
    assert_eq!(value["storedKeyAvailable"], false);
    assert!(value.get("password").is_none());
    assert!(value.get("recoveryPassword").is_none());
    assert!(value.get("plaintextFilesystem").is_none());
    assert!(value.get("recoveryPasswordReconstruction").is_none());
}

#[test]
fn recovery_password_reconstruction_serializes_transient_reveal() {
    let reconstruction = RecoveryPasswordReconstructionDto {
        status: "recovered".to_string(),
        password: Some("111111-222222-333333-444444-555555-666666-777777-888888".to_string()),
        volume_guid: Some("{GUID}".to_string()),
        protector_guid: Some("{PROTECTOR}".to_string()),
        reverse_datum_fingerprint: Some("abcdef0123456789".to_string()),
        reason: None,
    };
    let value = serde_json::to_value(&reconstruction).expect("reveal serializes");
    assert_eq!(value["status"], "recovered");
    assert_eq!(value["volumeGuid"], "{GUID}");
    assert!(value.get("reason").is_none());
    let round_trip: RecoveryPasswordReconstructionDto =
        serde_json::from_value(value).expect("reveal deserializes");
    assert_eq!(round_trip, reconstruction);

    let unavailable = RecoveryPasswordReconstructionDto {
        status: "unavailable".to_string(),
        password: None,
        volume_guid: None,
        protector_guid: None,
        reverse_datum_fingerprint: None,
        reason: Some("active VMK does not authenticate the reverse datum".to_string()),
    };
    let value = serde_json::to_value(&unavailable).expect("unavailable serializes");
    assert_eq!(value["status"], "unavailable");
    assert!(value.get("password").is_none());
}
