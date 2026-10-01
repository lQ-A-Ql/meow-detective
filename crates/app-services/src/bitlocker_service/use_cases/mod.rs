//! Application use-case entry points; implementation lives in one module per use case.
mod complete_unlock;
mod context;
mod inspect;
mod lock;
mod password;
mod recovery_password;
mod unlock;
mod unlock_audit;

use super::BitLockerServiceError;
pub(super) use complete_unlock::complete_verified_unlock;
pub(super) use context::{UnlockContext, UnlockMethod};
pub use inspect::inspect_bitlocker_volume;
pub use lock::lock_bitlocker_volume;
pub use password::unlock_bitlocker_with_password;
pub use recovery_password::unlock_bitlocker_with_recovery_password;
