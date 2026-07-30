use crate::bytes::read_guid;
use crate::metadata::{FveMetadata, MetadataEntry, PROTECTION_RECOVERY};

use super::error::RecoveryPasswordRecoveryError;

const VMK_FIXED_DATA_LEN: usize = 28;

/// Non-secret identity of one `0x0800` numerical recovery-password protector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecoveryPasswordProtectorIdentity {
    guid: [u8; 16],
}

impl RecoveryPasswordProtectorIdentity {
    #[must_use]
    pub fn from_guid(guid: [u8; 16]) -> Self {
        Self { guid }
    }

    #[must_use]
    pub fn guid(&self) -> [u8; 16] {
        self.guid
    }
}

/// Returns exact `0x0800` protector identities in on-disk order.
pub fn recovery_password_protectors(
    metadata: &FveMetadata,
) -> Result<Vec<RecoveryPasswordProtectorIdentity>, RecoveryPasswordRecoveryError> {
    metadata
        .vmk_entries()
        .filter(|entry| entry.protection_code() == Some(PROTECTION_RECOVERY))
        .map(identity_from_entry)
        .collect()
}

pub(super) fn select_protector(
    metadata: &FveMetadata,
    identity: RecoveryPasswordProtectorIdentity,
) -> Result<&MetadataEntry, RecoveryPasswordRecoveryError> {
    let mut matches = metadata.vmk_entries().filter(|entry| {
        entry.protection_code() == Some(PROTECTION_RECOVERY)
            && entry.data.get(..16) == Some(identity.guid.as_slice())
    });
    let selected = matches
        .next()
        .ok_or(RecoveryPasswordRecoveryError::ProtectorNotFound)?;
    if matches.next().is_some() {
        return Err(RecoveryPasswordRecoveryError::AmbiguousProtector);
    }
    Ok(selected)
}

fn identity_from_entry(
    entry: &MetadataEntry,
) -> Result<RecoveryPasswordProtectorIdentity, RecoveryPasswordRecoveryError> {
    if entry.data.len() < VMK_FIXED_DATA_LEN {
        return Err(RecoveryPasswordRecoveryError::MalformedProtector {
            reason: "VMK datum is shorter than its fixed header",
        });
    }
    Ok(RecoveryPasswordProtectorIdentity {
        guid: read_guid(&entry.data, 0),
    })
}
