use crate::EmulationError;

pub const LOGICAL_SECTOR_SIZE: u64 = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentIdentity {
    logical_length: u64,
    sha256: [u8; 32],
}

impl ParentIdentity {
    pub fn new(logical_length: u64, sha256: [u8; 32]) -> Result<Self, EmulationError> {
        if logical_length == 0 || !logical_length.is_multiple_of(LOGICAL_SECTOR_SIZE) {
            return Err(EmulationError::InvalidLogicalLength(logical_length));
        }
        Ok(Self {
            logical_length,
            sha256,
        })
    }

    pub fn logical_length(&self) -> u64 {
        self.logical_length
    }

    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
}
