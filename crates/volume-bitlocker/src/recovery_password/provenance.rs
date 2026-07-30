use sha2::{Digest, Sha256};

use crate::{format_guid, MetadataFingerprint, RecoveryPassword};

use super::protector::RecoveryPasswordProtectorIdentity;

/// Non-secret evidence binding for one recovered numerical password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryPasswordProvenance {
    volume_guid: String,
    protector_guid: String,
    metadata_fingerprint: String,
    reverse_datum_fingerprint: String,
}

impl RecoveryPasswordProvenance {
    pub(super) fn new(
        volume_guid: [u8; 16],
        protector: RecoveryPasswordProtectorIdentity,
        metadata_fingerprint: &MetadataFingerprint,
        reverse_datum: &[u8],
    ) -> Self {
        Self {
            volume_guid: format_guid(&volume_guid),
            protector_guid: format_guid(&protector.guid()),
            metadata_fingerprint: metadata_fingerprint.as_str().to_string(),
            reverse_datum_fingerprint: fingerprint(reverse_datum),
        }
    }

    #[must_use]
    pub fn volume_guid(&self) -> &str {
        &self.volume_guid
    }

    #[must_use]
    pub fn protector_guid(&self) -> &str {
        &self.protector_guid
    }

    #[must_use]
    pub fn metadata_fingerprint(&self) -> &str {
        &self.metadata_fingerprint
    }

    #[must_use]
    pub fn reverse_datum_fingerprint(&self) -> &str {
        &self.reverse_datum_fingerprint
    }
}

/// Secret recovery result paired with credential-free provenance.
pub struct RecoveredRecoveryPassword {
    password: RecoveryPassword,
    provenance: RecoveryPasswordProvenance,
}

impl RecoveredRecoveryPassword {
    pub(super) fn new(password: RecoveryPassword, provenance: RecoveryPasswordProvenance) -> Self {
        Self {
            password,
            provenance,
        }
    }

    #[must_use]
    pub fn password(&self) -> &RecoveryPassword {
        &self.password
    }

    #[must_use]
    pub fn provenance(&self) -> &RecoveryPasswordProvenance {
        &self.provenance
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(32);
    for byte in digest.iter().take(16) {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
