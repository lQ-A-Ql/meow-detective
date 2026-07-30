mod driver;
mod keyring;
mod object_directory;
mod production;
mod profile;
mod report;
mod volume_context;

pub use production::recover_vmks_structurally;
pub use profile::BitLockerMemoryProfile;
pub use report::BitLockerMemoryRecovery;

#[cfg(test)]
#[path = "../../tests/unit/keyring_recovery.rs"]
mod tests;
