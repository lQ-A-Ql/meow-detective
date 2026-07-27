use std::sync::Arc;

use app_services::bitlocker_service::BitLockerKeyStore;

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, ptr, slice};

    use app_services::bitlocker_service::{
        BitLockerKeyStore, BitLockerKeyStoreError, BitLockerKeyStoreOperation,
    };
    use volume_bitlocker::{MetadataFingerprint, PersistedKeyBlob};
    use windows::{
        core::{HRESULT, PCWSTR, PWSTR},
        Win32::{
            Foundation::{ERROR_NOT_FOUND, FILETIME},
            Security::Credentials::{
                CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
                CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
            },
        },
    };

    pub struct WindowsCredentialBitLockerKeyStore;

    impl BitLockerKeyStore for WindowsCredentialBitLockerKeyStore {
        fn load(
            &self,
            fingerprint: &MetadataFingerprint,
        ) -> Result<Option<PersistedKeyBlob>, BitLockerKeyStoreError> {
            let target = wide_null(&fingerprint.credential_target());
            let mut credential = ptr::null_mut();
            // SAFETY: `target` is NUL-terminated and remains alive for the call;
            // `credential` is an out pointer initialized to null and is owned by
            // `CredentialAllocation` only after a successful return.
            match unsafe {
                CredReadW(
                    PCWSTR(target.as_ptr()),
                    CRED_TYPE_GENERIC,
                    0,
                    &mut credential,
                )
            } {
                Ok(()) => read_blob(CredentialAllocation(credential)).map(Some),
                Err(error) if is_not_found(&error) => Ok(None),
                Err(error) => Err(platform_error(BitLockerKeyStoreOperation::Load, error)),
            }
        }

        fn store(
            &self,
            fingerprint: &MetadataFingerprint,
            blob: PersistedKeyBlob,
        ) -> Result<(), BitLockerKeyStoreError> {
            let mut target = wide_null(&fingerprint.credential_target());
            let mut comment = wide_null("Meow_Detective verified BitLocker volume key");
            let mut username = wide_null("Meow_Detective");
            let bytes = blob.expose_for_storage();
            let blob_len = u32::try_from(bytes.len()).map_err(|_| {
                BitLockerKeyStoreError::CorruptBlob(
                    volume_bitlocker::BitLockerError::PersistedKeyInvalid {
                        reason: "credential blob length exceeds the platform field",
                    },
                )
            })?;
            let credential = CREDENTIALW {
                Flags: CRED_FLAGS::default(),
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR(target.as_mut_ptr()),
                Comment: PWSTR(comment.as_mut_ptr()),
                LastWritten: FILETIME::default(),
                CredentialBlobSize: blob_len,
                CredentialBlob: bytes.as_ptr().cast_mut(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                AttributeCount: 0,
                Attributes: ptr::null_mut(),
                TargetAlias: PWSTR::null(),
                UserName: PWSTR(username.as_mut_ptr()),
            };
            // SAFETY: Every pointer in `credential` references a live buffer for
            // the duration of the synchronous call. CredWriteW copies the input
            // and does not take ownership of any pointer.
            unsafe { CredWriteW(&credential, 0) }
                .map_err(|error| platform_error(BitLockerKeyStoreOperation::Store, error))
        }

        fn delete(
            &self,
            fingerprint: &MetadataFingerprint,
        ) -> Result<bool, BitLockerKeyStoreError> {
            let target = wide_null(&fingerprint.credential_target());
            // SAFETY: `target` is a live NUL-terminated UTF-16 string and the API
            // does not retain it.
            match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0) } {
                Ok(()) => Ok(true),
                Err(error) if is_not_found(&error) => Ok(false),
                Err(error) => Err(platform_error(BitLockerKeyStoreOperation::Delete, error)),
            }
        }
    }

    struct CredentialAllocation(*mut CREDENTIALW);

    impl Drop for CredentialAllocation {
        fn drop(&mut self) {
            if self.0.is_null() {
                return;
            }
            // SAFETY: CredReadW allocated this CREDENTIALW and its blob. We keep
            // unique ownership, wipe exactly CredentialBlobSize bytes when the
            // pointer is non-null, then release the allocation exactly once with
            // CredFree as required by the API contract.
            unsafe {
                let credential = &mut *self.0;
                if !credential.CredentialBlob.is_null() && credential.CredentialBlobSize != 0 {
                    ptr::write_bytes(
                        credential.CredentialBlob,
                        0,
                        credential.CredentialBlobSize as usize,
                    );
                }
                CredFree(self.0.cast::<c_void>());
            }
        }
    }

    fn read_blob(
        allocation: CredentialAllocation,
    ) -> Result<PersistedKeyBlob, BitLockerKeyStoreError> {
        if allocation.0.is_null() {
            return Err(platform_failure(BitLockerKeyStoreOperation::Load));
        }
        // SAFETY: `allocation` owns a live CREDENTIALW returned by CredReadW.
        // The pointer and reported blob span remain valid until allocation drops.
        let credential = unsafe { &*allocation.0 };
        if credential.CredentialBlob.is_null() {
            return Err(BitLockerKeyStoreError::CorruptBlob(
                volume_bitlocker::BitLockerError::PersistedKeyInvalid {
                    reason: "credential blob pointer is null",
                },
            ));
        }
        // SAFETY: CredReadW guarantees CredentialBlob points to at least
        // CredentialBlobSize bytes inside its returned allocation.
        let bytes = unsafe {
            slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        }
        .to_vec();
        PersistedKeyBlob::from_storage(bytes).map_err(BitLockerKeyStoreError::CorruptBlob)
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn is_not_found(error: &windows::core::Error) -> bool {
        error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0)
    }

    fn platform_error(
        operation: BitLockerKeyStoreOperation,
        error: windows::core::Error,
    ) -> BitLockerKeyStoreError {
        BitLockerKeyStoreError::Platform {
            operation,
            system_code: error.code().0,
        }
    }

    fn platform_failure(operation: BitLockerKeyStoreOperation) -> BitLockerKeyStoreError {
        BitLockerKeyStoreError::Platform {
            operation,
            system_code: -1,
        }
    }
}

#[cfg(windows)]
pub fn platform_bitlocker_key_store() -> Arc<dyn BitLockerKeyStore> {
    Arc::new(platform::WindowsCredentialBitLockerKeyStore)
}

#[cfg(not(windows))]
pub fn platform_bitlocker_key_store() -> Arc<dyn BitLockerKeyStore> {
    Arc::new(UnsupportedBitLockerKeyStore)
}

#[cfg(not(windows))]
struct UnsupportedBitLockerKeyStore;

#[cfg(not(windows))]
impl BitLockerKeyStore for UnsupportedBitLockerKeyStore {
    fn load(
        &self,
        _fingerprint: &volume_bitlocker::MetadataFingerprint,
    ) -> Result<
        Option<volume_bitlocker::PersistedKeyBlob>,
        app_services::bitlocker_service::BitLockerKeyStoreError,
    > {
        Err(app_services::bitlocker_service::BitLockerKeyStoreError::Unsupported)
    }

    fn store(
        &self,
        _fingerprint: &volume_bitlocker::MetadataFingerprint,
        _blob: volume_bitlocker::PersistedKeyBlob,
    ) -> Result<(), app_services::bitlocker_service::BitLockerKeyStoreError> {
        Err(app_services::bitlocker_service::BitLockerKeyStoreError::Unsupported)
    }

    fn delete(
        &self,
        _fingerprint: &volume_bitlocker::MetadataFingerprint,
    ) -> Result<bool, app_services::bitlocker_service::BitLockerKeyStoreError> {
        Err(app_services::bitlocker_service::BitLockerKeyStoreError::Unsupported)
    }
}

#[cfg(test)]
#[path = "../tests/unit/bitlocker_key_store.rs"]
mod tests;
