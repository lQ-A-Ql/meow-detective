mod evidence_reader;
mod registry;

pub use evidence_reader::{open_registered_bitlocker_volume, BitLockerEvidenceReader};
pub use registry::{
    BitLockerRuntimeError, BitLockerUnlockRegistry, BitLockerVolumeScope, RegisteredBitLockerVolume,
};

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_runtime.rs"]
mod tests;
