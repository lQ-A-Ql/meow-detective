use super::key_store;

#[derive(Clone, Copy)]
pub struct BitLockerRuntimeContext<'a> {
    pub(super) preview_runtime: &'a std::sync::Arc<crate::file_service::PreviewRuntimeRegistry>,
    pub(super) bitlocker_runtime:
        &'a std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    pub(super) key_store: &'a dyn key_store::BitLockerKeyStore,
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

    pub(crate) fn unlock_registry(
        self,
    ) -> &'a std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry> {
        self.bitlocker_runtime
    }
}
