use transport::ServiceErrorCategory;
use volume_bitlocker::{
    restore_volume_from_persisted_key, EncryptionMethod, FveMetadata, MetadataEntry,
    MetadataFingerprint, PersistedKeyBlob, VolumeIdentity,
};

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
    let status = status::build_status("source-1", 2, &identity, 3, false, false, None);

    assert!(!status.unlocked);
    assert!(status.supports_password);
    assert!(status.supports_recovery_password);
    assert_eq!(status.protectors.len(), 2);
    assert_eq!(status.protectors[0].kind, "password");
    assert_eq!(status.protectors[1].kind, "recoveryPassword");
    assert_eq!(status.metadata_copy_count, 3);
    assert!(!status.stored_key_available);
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

#[test]
fn persisted_key_fingerprint_mismatch_is_nonrecoverable_validation() {
    let error = BitLockerServiceError::PersistedKeyFingerprintMismatch;

    assert_eq!(
        error.code(),
        Some("BITLOCKER_PERSISTED_KEY_FINGERPRINT_MISMATCH")
    );
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Validation
    ));
    assert_eq!(error.recoverable(), Some(false));
}

#[test]
fn memory_candidate_failure_has_a_stable_secret_free_command_contract() {
    let error = BitLockerServiceError::MemoryKeyNotValidated;

    assert_eq!(error.code(), Some("BITLOCKER_MEMORY_KEY_NOT_VALIDATED"));
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Security
    ));
    assert_eq!(error.recoverable(), Some(true));
    assert!(error.safe_details().is_none());
    assert!(error.suggestion().is_some());

    let command_error = transport::CommandError::from_typed_service_error(error);
    assert_eq!(command_error.code, "BITLOCKER_MEMORY_KEY_NOT_VALIDATED");
    assert_eq!(command_error.category, "security");
    assert!(command_error.details.is_none());
    assert!(!command_error
        .message
        .to_ascii_lowercase()
        .contains("key bytes"));
}

#[test]
fn unreviewed_memory_build_is_typed_unsupported() {
    let error = BitLockerServiceError::MemoryImage(
        memory_windows::MemoryWindowsError::TargetedKernelIdentityMismatch {
            expected_timestamp: 1,
            expected_size: 2,
            actual_timestamp: 3,
            actual_size: 4,
        },
    );

    assert_eq!(error.code(), Some("BITLOCKER_MEMORY_PROFILE_UNSUPPORTED"));
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Unsupported
    ));
    assert_eq!(error.recoverable(), Some(false));
    assert!(error.safe_details().is_none());
}

#[derive(Default)]
struct TestKeyStore {
    blobs: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

impl BitLockerKeyStore for TestKeyStore {
    fn load(
        &self,
        fingerprint: &MetadataFingerprint,
    ) -> Result<Option<PersistedKeyBlob>, BitLockerKeyStoreError> {
        let bytes = self
            .blobs
            .lock()
            .expect("test key store lock")
            .get(fingerprint.as_str())
            .cloned();
        bytes
            .map(PersistedKeyBlob::from_storage)
            .transpose()
            .map_err(BitLockerKeyStoreError::CorruptBlob)
    }

    fn store(
        &self,
        fingerprint: &MetadataFingerprint,
        blob: PersistedKeyBlob,
    ) -> Result<(), BitLockerKeyStoreError> {
        self.blobs.lock().expect("test key store lock").insert(
            fingerprint.as_str().to_string(),
            blob.expose_for_storage().to_vec(),
        );
        Ok(())
    }

    fn delete(&self, fingerprint: &MetadataFingerprint) -> Result<bool, BitLockerKeyStoreError> {
        Ok(self
            .blobs
            .lock()
            .expect("test key store lock")
            .remove(fingerprint.as_str())
            .is_some())
    }
}

fn persisted_envelope(identity: &VolumeIdentity) -> Vec<u8> {
    let fingerprint = MetadataFingerprint::from_metadata(&identity.metadata);
    let fvek_len = identity
        .metadata
        .encryption_method
        .fvek_len()
        .expect("test method is decryptable");
    let mut bytes = Vec::with_capacity(48 + fvek_len);
    bytes.extend_from_slice(b"MEOWBLK1");
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&identity.metadata.encryption_method_code.to_le_bytes());
    bytes.extend_from_slice(fingerprint.as_str().as_bytes());
    bytes.extend_from_slice(&(fvek_len as u16).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend(std::iter::repeat_n(0x5a, fvek_len));
    bytes
}

#[test]
fn runtime_lock_and_persisted_key_forget_are_independent() {
    let identity = identity_with_password_and_recovery();
    let fingerprint = MetadataFingerprint::from_metadata(&identity.metadata);
    let bytes = persisted_envelope(&identity);
    let store = TestKeyStore::default();
    store
        .store(
            &fingerprint,
            PersistedKeyBlob::from_storage(bytes.clone()).expect("valid envelope"),
        )
        .expect("store key");
    let registry = crate::bitlocker_runtime::BitLockerUnlockRegistry::default();
    let verified = restore_volume_from_persisted_key(
        identity.clone(),
        PersistedKeyBlob::from_storage(bytes.clone()).expect("valid envelope"),
    )
    .expect("restore verified state");
    registry
        .register_verified("case-1", "source-1", 2, verified)
        .expect("register runtime");

    registry
        .invalidate_partition("case-1", "source-1", 2)
        .expect("lock runtime");
    assert!(store.contains(&fingerprint).expect("stored key remains"));

    let verified = restore_volume_from_persisted_key(
        identity.clone(),
        PersistedKeyBlob::from_storage(bytes).expect("valid envelope"),
    )
    .expect("restore verified state");
    registry
        .register_verified("case-1", "source-1", 2, verified)
        .expect("register runtime");
    assert!(store.delete(&fingerprint).expect("forget key"));
    assert!(registry
        .resolve_for_identities("case-1", "source-1", 2, &[identity])
        .is_ok());
}
