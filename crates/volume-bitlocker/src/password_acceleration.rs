//! Batch KDF boundary for optional compute devices. No volume keys are exposed.
use zeroize::Zeroizing;

use crate::kdf::password_hash;
use crate::metadata::PROTECTION_PASSWORD;
use crate::unlock::{checked_fvek_len, retain_preferred_error, stretch_salt_for_protector};
use crate::{BitLockerError, Passphrase, ProtectorKind, Result, VerifiedUnlock, VolumeIdentity};

/// Immutable target metadata and distinct password salts for a batch accelerator.
pub struct PasswordAccelerationTarget {
    identities: Vec<VolumeIdentity>,
    salts: Vec<[u8; 16]>,
}

impl PasswordAccelerationTarget {
    pub fn new(identities: &[VolumeIdentity]) -> Result<Self> {
        let mut salts = Vec::new();
        let mut preferred_error = None;
        for identity in identities {
            let salt = checked_fvek_len(&identity.metadata).and_then(|_| {
                stretch_salt_for_protector(
                    &identity.metadata,
                    ProtectorKind::Password,
                    PROTECTION_PASSWORD,
                )
            });
            match salt {
                Ok(salt) if !salts.contains(&salt) => salts.push(salt),
                Ok(_) => {}
                Err(error) => retain_preferred_error(&mut preferred_error, error),
            }
        }
        if salts.is_empty() {
            return Err(preferred_error.unwrap_or(BitLockerError::CredentialRejected));
        }
        Ok(Self {
            identities: identities.to_vec(),
            salts,
        })
    }

    pub fn salts(&self) -> &[[u8; 16]] {
        &self.salts
    }

    /// Credential hashes as SHA-256 big-endian words; memory is erased on drop.
    pub fn initial_hashes(passwords: &[Passphrase]) -> Zeroizing<Vec<[u32; 8]>> {
        Zeroizing::new(
            passwords
                .iter()
                .map(|password| {
                    let hash = password_hash(password.expose_for_derivation());
                    std::array::from_fn(|i| {
                        u32::from_be_bytes([
                            hash[i * 4],
                            hash[i * 4 + 1],
                            hash[i * 4 + 2],
                            hash[i * 4 + 3],
                        ])
                    })
                })
                .collect(),
        )
    }

    /// Authenticate device output against each metadata copy's VMK and FVEK.
    /// The application additionally repeats a found candidate with the CPU KDF.
    pub fn verify(&self, derived: &[[u8; 32]]) -> Result<VerifiedUnlock> {
        if derived.len() != self.salts.len() {
            return Err(BitLockerError::CredentialRejected);
        }
        crate::unlock_dictionary::unlock_identities_with_stretch(
            &self.identities,
            ProtectorKind::Password,
            PROTECTION_PASSWORD,
            |salt| {
                let key = self
                    .salts
                    .iter()
                    .position(|value| value == salt)
                    .and_then(|index| derived.get(index))
                    .copied()
                    .unwrap_or_default();
                Zeroizing::new(key)
            },
        )
    }
}

#[cfg(test)]
#[path = "../tests/unit/password_acceleration.rs"]
mod tests;
