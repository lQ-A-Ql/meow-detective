use zeroize::Zeroizing;

use crate::kdf::aes_ccm_unwrap;
use crate::metadata::{MetadataEntry, VALUE_TYPE_AES_CCM, VALUE_TYPE_STRETCH};
use crate::RecoveredVmk;

use super::error::RecoveryPasswordRecoveryError;

const VMK_NESTED_OFFSET: usize = 28;
const STRETCH_NESTED_OFFSET: usize = 20;
const REVERSE_DATUM_LEN: usize = 12 + 16 + 28;

pub(super) struct ReverseRecoveryDatum {
    value_data: Vec<u8>,
}

impl ReverseRecoveryDatum {
    pub(super) fn from_protector(
        protector: &MetadataEntry,
    ) -> Result<Self, RecoveryPasswordRecoveryError> {
        let properties = protector.nested_exact(VMK_NESTED_OFFSET).ok_or(
            RecoveryPasswordRecoveryError::MalformedProtector {
                reason: "VMK nested datum sequence is truncated",
            },
        )?;
        let stretch = exactly_one(&properties, VALUE_TYPE_STRETCH, "stretch-key datum")?;
        let nested = stretch.nested_exact(STRETCH_NESTED_OFFSET).ok_or(
            RecoveryPasswordRecoveryError::MalformedProtector {
                reason: "stretch-key nested datum sequence is truncated",
            },
        )?;
        let reverse = exactly_one(&nested, VALUE_TYPE_AES_CCM, "reverse AES-CCM datum")?;
        if reverse.data.len() != REVERSE_DATUM_LEN {
            return Err(RecoveryPasswordRecoveryError::MalformedProtector {
                reason: "reverse AES-CCM datum must contain a 12-byte nonce, 16-byte tag, and 28-byte ciphertext",
            });
        }
        Ok(Self {
            value_data: reverse.data.clone(),
        })
    }

    pub(super) fn authenticate(
        &self,
        vmk: &RecoveredVmk,
    ) -> Result<Zeroizing<Vec<u8>>, RecoveryPasswordRecoveryError> {
        aes_ccm_unwrap(vmk.expose_for_recovery(), &self.value_data)
            .ok_or(RecoveryPasswordRecoveryError::AuthenticationFailed)
    }

    pub(super) fn encoded(&self) -> &[u8] {
        &self.value_data
    }
}

fn exactly_one<'a>(
    entries: &'a [MetadataEntry],
    value_type: u16,
    label: &'static str,
) -> Result<&'a MetadataEntry, RecoveryPasswordRecoveryError> {
    let mut matches = entries
        .iter()
        .filter(|entry| entry.value_type == value_type);
    let selected = matches
        .next()
        .ok_or(RecoveryPasswordRecoveryError::MalformedProtector { reason: label })?;
    if matches.next().is_some() {
        return Err(RecoveryPasswordRecoveryError::MalformedProtector {
            reason: "a required nested datum is duplicated",
        });
    }
    Ok(selected)
}
