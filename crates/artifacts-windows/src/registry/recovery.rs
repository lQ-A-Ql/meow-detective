//! Deleted cell recovery for Windows registry hives.

mod bytes;
mod constants;
mod records;
mod scanner;
mod types;

pub use scanner::scan_deleted_registry_cells;
pub(crate) use types::FreeCell;
pub use types::{RecoverResult, RecoveredKey, RecoveredValue};

#[cfg(test)]
#[path = "../../tests/unit/registry/recovery.rs"]
mod tests;
