use transport::ServiceErrorCategory;
use volume_bitlocker::{EncryptionMethod, FveMetadata, MetadataEntry, VolumeIdentity};

use super::*;

fn identity_with_password_and_recovery() -> VolumeIdentity {
    let mut password = vec![0u8; 28];
    password[26..28].copy_from_slice(&0x2000u16.to_le_bytes());
    let mut recovery = vec![0u8; 28];
    recovery[26..28].copy_from_slice(&0x0800u16.to_le_bytes());
    VolumeIdentity {
        metadata: FveMetadata {
            encryption_method: EncryptionMethod::XtsAes256,
            encryption_method_code: 0x8005,
            volume_guid: [0x42; 16],
            creation_time: 123,
            entries: vec![
                MetadataEntry {
                    entry_type: 0x0002,
                    value_type: 0x0008,
                    version: 1,
                    data: password,
                },
                MetadataEntry {
                    entry_type: 0x0002,
                    value_type: 0x0008,
                    version: 1,
                    data: recovery,
                },
            ],
            encrypted_volume_size: 1024,
            volume_header_offset: 0,
            volume_header_size: 512,
            metadata_offsets: [4096, 8192, 12288],
            metadata_size: 128,
        },
        bytes_per_sector: 512,
    }
}

#[test]
fn status_reports_non_secret_unlock_capabilities() {
    let identity = identity_with_password_and_recovery();
    let status = status::build_status("source-1", 2, &identity, 3, false, None);

    assert!(!status.unlocked);
    assert!(status.supports_password);
    assert!(status.supports_recovery_password);
    assert_eq!(status.protectors.len(), 2);
    assert_eq!(status.protectors[0].kind, "password");
    assert_eq!(status.protectors[1].kind, "recoveryPassword");
    assert_eq!(status.metadata_copy_count, 3);
    assert!(status.plaintext_filesystem.is_none());
}

#[test]
fn rejected_credentials_keep_the_stable_security_contract() {
    let error = BitLockerServiceError::Volume(volume_bitlocker::BitLockerError::CredentialRejected);

    assert_eq!(error.code(), Some("BITLOCKER_CREDENTIAL_REJECTED"));
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Security
    ));
    assert_eq!(error.recoverable(), Some(true));
}
