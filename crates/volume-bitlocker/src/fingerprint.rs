//! Stable identity for an encrypted volume.
//!
//! The fingerprint keys both the Credential Manager target
//! (`Meow_Detective/BitLocker/v1/<fingerprint>`) and the runtime key registry, so
//! it has two hard requirements:
//!
//! - **Stable across reads and across cases.** Reopening the same evidence must
//!   produce the same fingerprint, or a stored key package becomes unreachable.
//! - **Derived only from metadata that identifies the volume**, never from a
//!   credential, a path, or a case identifier. Paths change when evidence moves,
//!   and mixing in a credential would leak guessable material into a name that
//!   gets persisted.

use sha2::{Digest, Sha256};

use crate::metadata::FveMetadata;

/// A stable, credential-free identifier for an encrypted volume.
///
/// `Clone` and `Debug` are allowed here — unlike the secret types — precisely
/// because this carries no key material. It is meant to appear in logs and
/// diagnostics, which is how a stored key package gets traced back to a volume.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetadataFingerprint(String);

impl MetadataFingerprint {
    /// Derives the fingerprint from parsed FVE metadata.
    ///
    /// Inputs are the volume GUID, creation time, encryption-method code, and the
    /// protector codes in on-disk order. The protector set is included so that
    /// adding or removing a protector — which changes what can unlock the volume —
    /// yields a different fingerprint rather than silently reusing a stored key
    /// package for a volume that has since been re-keyed.
    #[must_use]
    pub fn from_metadata(metadata: &FveMetadata) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"Meow_Detective/BitLocker/v1");
        hasher.update(metadata.volume_guid);
        hasher.update(metadata.creation_time.to_le_bytes());
        hasher.update(metadata.encryption_method_code.to_le_bytes());
        for code in metadata.protector_codes() {
            hasher.update(code.to_le_bytes());
        }
        let digest = hasher.finalize();
        // 32 hex characters is 128 bits: far past collision concerns for a
        // per-machine credential namespace, and short enough to read in a log line.
        let mut hex = String::with_capacity(32);
        for byte in digest.iter().take(16) {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
        }
        Self(hex)
    }

    /// The fingerprint as a hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The Credential Manager target for this volume's key package.
    #[must_use]
    pub fn credential_target(&self) -> String {
        format!("Meow_Detective/BitLocker/v1/{}", self.0)
    }
}

#[cfg(test)]
#[path = "../tests/unit/fingerprint.rs"]
mod tests;
