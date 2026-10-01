//! Preserve credential and unsupported errors when metadata fallback fails.
use crate::BitLockerError;
pub(crate) fn retain_preferred_error(slot: &mut Option<BitLockerError>, candidate: BitLockerError) {
    let candidate_rank = unlock_error_rank(&candidate);
    let current_rank = slot.as_ref().map(unlock_error_rank).unwrap_or(0);
    if candidate_rank >= current_rank {
        *slot = Some(candidate);
    }
}

fn unlock_error_rank(error: &BitLockerError) -> u8 {
    match error {
        BitLockerError::CredentialRejected => 4,
        BitLockerError::UnsupportedProtector { .. } => 3,
        BitLockerError::UnsupportedEncryptionMethod { .. } => 2,
        _ => 1,
    }
}
