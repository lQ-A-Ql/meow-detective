mod audit;
mod catalog;
mod error;
mod source;
mod status;
mod use_cases;

#[derive(Clone, Copy)]
pub struct BitLockerRuntimeContext<'a> {
    preview_runtime: &'a std::sync::Arc<crate::file_service::PreviewRuntimeRegistry>,
    bitlocker_runtime: &'a std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
}

impl<'a> BitLockerRuntimeContext<'a> {
    #[must_use]
    pub fn new(
        preview_runtime: &'a std::sync::Arc<crate::file_service::PreviewRuntimeRegistry>,
        bitlocker_runtime: &'a std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    ) -> Self {
        Self {
            preview_runtime,
            bitlocker_runtime,
        }
    }
}

pub use catalog::import_unlocked_bitlocker_catalog;
pub use error::BitLockerServiceError;
pub use use_cases::{
    inspect_bitlocker_volume, lock_bitlocker_volume, unlock_bitlocker_with_password,
    unlock_bitlocker_with_recovery_password,
};

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_service.rs"]
mod tests;
