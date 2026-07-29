use super::*;
use crate::bitlocker_service::{BitLockerKeyStoreError, BitLockerKeyStoreOperation};

#[test]
fn permanent_restore_failures_disable_future_attempts() {
    for error in [
        BitLockerServiceError::StoredKeyNotFound,
        BitLockerServiceError::PersistedKeyFingerprintMismatch,
        BitLockerServiceError::NotBitLocker { partition_index: 4 },
        BitLockerServiceError::KeyStore(BitLockerKeyStoreError::Unsupported),
        BitLockerServiceError::Volume(volume_bitlocker::BitLockerError::PersistedKeyInvalid {
            reason: "test fixture",
        }),
    ] {
        assert_eq!(
            restore_failure_status(&error),
            BitLockerRestoreStatus::Disabled
        );
    }
}

#[test]
fn retryable_restore_failures_remain_enabled() {
    let error = BitLockerServiceError::KeyStore(BitLockerKeyStoreError::Platform {
        operation: BitLockerKeyStoreOperation::Load,
        system_code: 5,
    });

    assert_eq!(
        restore_failure_status(&error),
        BitLockerRestoreStatus::Failed
    );
    assert_eq!(
        stable_error_code(&error),
        Some("BITLOCKER_KEY_STORE_FAILED")
    );
}
