mod activation;
mod audit;
mod catalog;
mod error;
mod key_store;
mod persistence;
mod source;
mod status;
mod use_cases;

#[derive(Clone, Copy)]
pub struct BitLockerRuntimeContext<'a> {
    preview_runtime: &'a std::sync::Arc<crate::file_service::PreviewRuntimeRegistry>,
    bitlocker_runtime: &'a std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    key_store: &'a dyn key_store::BitLockerKeyStore,
}

impl<'a> BitLockerRuntimeContext<'a> {
    #[must_use]
    pub fn new(
        preview_runtime: &'a std::sync::Arc<crate::file_service::PreviewRuntimeRegistry>,
        bitlocker_runtime: &'a std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
        key_store: &'a dyn key_store::BitLockerKeyStore,
    ) -> Self {
        Self {
            preview_runtime,
            bitlocker_runtime,
            key_store,
        }
    }
}

pub use catalog::import_unlocked_bitlocker_catalog;
pub use error::BitLockerServiceError;
pub use key_store::{BitLockerKeyStore, BitLockerKeyStoreError, BitLockerKeyStoreOperation};
pub use persistence::{forget_persisted_bitlocker_key, restore_persisted_bitlocker_key};
pub use use_cases::{
    inspect_bitlocker_volume, lock_bitlocker_volume, unlock_bitlocker_with_password,
    unlock_bitlocker_with_recovery_password,
};

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_service.rs"]
mod tests;
