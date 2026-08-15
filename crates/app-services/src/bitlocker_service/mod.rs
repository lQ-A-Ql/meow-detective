mod activation;
mod audit;
mod catalog;
mod context;
mod error;
mod key_store;
mod memory_recovery;
mod persistence;
mod report;
mod restore_on_open;
mod source;
mod status;
mod use_cases;

pub use catalog::import_unlocked_bitlocker_catalog;
pub use context::BitLockerRuntimeContext;
pub use error::BitLockerServiceError;
pub use key_store::{BitLockerKeyStore, BitLockerKeyStoreError, BitLockerKeyStoreOperation};
pub use memory_recovery::unlock_bitlocker_with_memory_image;
pub use persistence::{forget_persisted_bitlocker_key, restore_persisted_bitlocker_key};
pub(crate) use report::{collect_report_inventory, BitLockerReportEntry};
pub use restore_on_open::{restore_enabled_bitlocker_volumes, BitLockerRestoreSummary};
pub use use_cases::{
    inspect_bitlocker_volume, lock_bitlocker_volume, unlock_bitlocker_with_password,
    unlock_bitlocker_with_recovery_password,
};

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_service.rs"]
mod tests;
