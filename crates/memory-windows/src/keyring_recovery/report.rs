use volume_bitlocker::RecoveredVmk;

use crate::PhysicalReadStats;

/// Structurally sourced VMKs from one exact kernel/fvevol profile.
///
/// The VMKs are not volume-authenticated at this layer. The caller must bind
/// them to target BitLocker metadata and fail closed unless exactly one passes.
/// This type intentionally has no `Debug`, `Clone`, or serialization support.
pub struct BitLockerMemoryRecovery {
    vmks: Vec<RecoveredVmk>,
    profile_id: String,
    build_id: String,
    keyring_datasets_examined: usize,
    devices_examined: usize,
    datum_pointers_examined: usize,
    physical_reads: PhysicalReadStats,
}

impl BitLockerMemoryRecovery {
    pub(crate) fn new(
        vmks: Vec<RecoveredVmk>,
        profile_id: String,
        build_id: String,
        keyring_datasets_examined: usize,
        devices_examined: usize,
        datum_pointers_examined: usize,
        physical_reads: PhysicalReadStats,
    ) -> Self {
        Self {
            vmks,
            profile_id,
            build_id,
            keyring_datasets_examined,
            devices_examined,
            datum_pointers_examined,
            physical_reads,
        }
    }

    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    #[must_use]
    pub fn build_id(&self) -> &str {
        &self.build_id
    }

    #[must_use]
    pub fn recovered_vmk_count(&self) -> usize {
        self.vmks.len()
    }

    #[must_use]
    pub fn keyring_datasets_examined(&self) -> usize {
        self.keyring_datasets_examined
    }

    #[must_use]
    pub fn devices_examined(&self) -> usize {
        self.devices_examined
    }

    #[must_use]
    pub fn datum_pointers_examined(&self) -> usize {
        self.datum_pointers_examined
    }

    #[must_use]
    pub fn physical_reads(&self) -> PhysicalReadStats {
        self.physical_reads
    }

    #[must_use]
    pub fn into_vmks(self) -> Vec<RecoveredVmk> {
        self.vmks
    }
}
