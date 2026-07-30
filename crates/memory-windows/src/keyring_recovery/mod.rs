mod driver;
mod keyring;
mod object_directory;
mod production;
mod profile;
mod report;
mod symbol_registry_generated;
mod symbol_table;
mod volume_context;

pub use production::{recover_vmks_structurally, resolve_profile_for_image};
pub use profile::BitLockerMemoryProfile;
pub use report::BitLockerMemoryRecovery;

#[cfg(test)]
#[path = "../../tests/unit/keyring_recovery.rs"]
mod tests;
