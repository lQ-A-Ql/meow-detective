use thiserror::Error;

/// A bounded request for bytes from one canonical RBD data object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbdObjectReadRequest {
    pub object_no: u64,
    pub object_identity: String,
    pub object_offset: u64,
    pub length: usize,
}

/// The provider writes present bytes directly into the supplied output slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RbdObjectReadOutcome {
    Present {
        object_identity: String,
        bytes_read: usize,
    },
    Missing,
}

#[derive(Debug, Error)]
pub enum RbdObjectProviderError {
    #[error("RBD object provider is unavailable for {object_identity}: {reason}")]
    Unavailable {
        object_identity: String,
        reason: String,
    },
    #[error("RBD object range read failed for {object_identity}: {reason}")]
    ReadFailed {
        object_identity: String,
        reason: String,
    },
}

/// Resolves canonical RBD data objects without exposing or guessing host paths.
pub trait RbdObjectProvider: Send {
    fn read_object_range(
        &mut self,
        request: &RbdObjectReadRequest,
        output: &mut [u8],
    ) -> Result<RbdObjectReadOutcome, RbdObjectProviderError>;
}
