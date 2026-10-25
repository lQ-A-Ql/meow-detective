pub mod browser;
pub mod error;
pub mod hash_decrypt;
pub(crate) mod hash_encrypt;
pub mod log_replay;
pub mod lookup;
pub mod recovery;
pub mod sam_edit;
pub mod sam_structs;
pub mod snapshot;
pub mod util;

#[cfg(test)]
#[path = "../../tests/unit/registry/mod.rs"]
mod tests;

pub use error::RegistryError;
pub use recovery::{scan_deleted_registry_cells, RecoverResult, RecoveredKey, RecoveredValue};
pub use snapshot::RegistrySnapshot;
