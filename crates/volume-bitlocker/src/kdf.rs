//! BitLocker key derivation and AES-CCM key unwrap.
//!
//! Derived from `bitlocker-core`'s `crypto` module (see `../NOTICE`).
//!
//! Every primitive comes from an audited RustCrypto crate: `sha2`, `aes`, `ccm`.
//!
//! # This is not DPAPI
//!
//! BitLocker's credential path is native to the volume format: UTF-16LE encode,
//! double SHA-256, a fixed 0x100000-iteration stretch against a salt from the
//! metadata, then AES-CCM to unwrap the volume master key. DPAPI protects
//! Windows user-profile secrets and has no part in it.

use aes::Aes256;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::AeadInPlace;
use ccm::consts::{U12, U16};
use ccm::{Ccm, KeyInit};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// Stretch iterations mandated by the BitLocker format.
pub(crate) const STRETCH_ITERATIONS: u64 = 0x0010_0000;

/// AES-256-CCM with a 16-byte tag and 12-byte nonce, the mode BitLocker uses to
/// wrap both the VMK and the FVEK. The type parameters are tag size then nonce size.
type BitLockerCcm = Ccm<Aes256, U16, U12>;

/// Computes `SHA-256(SHA-256(UTF-16LE(password)))`.
///
/// No byte-order mark and no NUL terminator; the encoding is of the password
/// characters alone.
#[must_use]
pub(crate) fn password_hash(password: &str) -> Zeroizing<[u8; 32]> {
    let utf16: Zeroizing<Vec<u8>> =
        Zeroizing::new(password.encode_utf16().flat_map(u16::to_le_bytes).collect());
    let first = Sha256::digest(utf16.as_slice());
    Zeroizing::new(Sha256::digest(first).into())
}

/// Runs the stretch loop `iterations` times.
///
/// The hashed structure is `last(32) | initial(32) | salt(16) | count(u64 LE)`.
/// Each round hashes it into `last` and increments `count`.
///
/// The count is a parameter only so tests can exercise the orchestration cheaply.
/// Every production caller passes [`STRETCH_ITERATIONS`]; the entry points that
/// accept anything else are `cfg(test)`.
#[must_use]
pub(crate) fn stretch_key_n(
    password_hash: &[u8; 32],
    salt: &[u8; 16],
    iterations: u64,
) -> Zeroizing<[u8; 32]> {
    let mut buffer = Zeroizing::new([0u8; 88]);
    buffer[32..64].copy_from_slice(password_hash);
    buffer[64..80].copy_from_slice(salt);
    for count in 0..iterations {
        buffer[80..88].copy_from_slice(&count.to_le_bytes());
        let digest = Sha256::digest(buffer.as_slice());
        buffer[0..32].copy_from_slice(&digest);
    }
    let mut out = Zeroizing::new([0u8; 32]);
    out.copy_from_slice(&buffer[0..32]);
    out
}

/// Why a 48-digit recovery password was rejected.
///
/// Each variant names the structural rule that failed and never echoes the input,
/// because this reason string reaches error details and therefore logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPasswordError {
    /// Not exactly eight groups.
    GroupCount,
    /// A group was not exactly six ASCII digits.
    GroupShape,
    /// A group failed the divisible-by-eleven checksum.
    Checksum,
    /// A group divided by eleven did not fit in 16 bits.
    OutOfRange,
}

impl RecoveryPasswordError {
    /// A stable, credential-free explanation.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Self::GroupCount => "recovery password must be exactly 8 groups",
            Self::GroupShape => "each recovery group must be exactly 6 digits",
            Self::Checksum => "a recovery group failed the divisible-by-11 checksum",
            Self::OutOfRange => "a recovery group is out of range (value / 11 exceeds 16 bits)",
        }
    }
}

/// Derives the stretch input from a 48-digit recovery password.
///
/// The password is eight groups of six digits. Each group must divide by eleven —
/// that is its checksum — and the quotient must fit in 16 bits. Those eight
/// little-endian words form a 16-byte binary key whose SHA-256 is the hash fed to
/// [`stretch_key`], the recovery analogue of [`password_hash`].
///
/// Validating the checksum before deriving matters: a typo that slips through
/// would silently produce a wrong key, which is indistinguishable from a wrong
/// password at the AES-CCM tag check.
///
/// # Errors
///
/// [`RecoveryPasswordError`] when the structure is malformed.
pub(crate) fn recovery_key_hash(
    recovery: &str,
) -> std::result::Result<Zeroizing<[u8; 32]>, RecoveryPasswordError> {
    let mut key = Zeroizing::new([0u8; 16]);
    let mut groups = 0usize;
    for (index, group) in recovery.split('-').enumerate() {
        if index >= 8 {
            return Err(RecoveryPasswordError::GroupCount);
        }
        if group.len() != 6 || !group.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(RecoveryPasswordError::GroupShape);
        }
        // Six ASCII digits always fit in u32.
        let value: u32 = group
            .parse()
            .map_err(|_| RecoveryPasswordError::GroupShape)?;
        if !value.is_multiple_of(11) {
            return Err(RecoveryPasswordError::Checksum);
        }
        let word = value / 11;
        if word > u32::from(u16::MAX) {
            return Err(RecoveryPasswordError::OutOfRange);
        }
        key[index * 2..index * 2 + 2].copy_from_slice(&(word as u16).to_le_bytes());
        groups = index + 1;
    }
    if groups != 8 {
        return Err(RecoveryPasswordError::GroupCount);
    }
    Ok(Zeroizing::new(Sha256::digest(key.as_slice()).into()))
}

/// AES-CCM-unwraps a key.
///
/// `value_data` is an AES-CCM wrapped-key value: `nonce(12) | tag(16) |
/// ciphertext`. `key` is the 256-bit unwrapping key — the stretched credential for
/// the VMK, the VMK for the FVEK.
///
/// Returns `None` when the authentication tag does not verify. That is the
/// wrong-credential signal, and it is deliberately indistinguishable from any
/// other tag failure: the tag check is the only thing standing between a guess
/// and the plaintext, so it must not leak how close the guess was.
#[must_use]
pub(crate) fn aes_ccm_unwrap(key: &[u8; 32], value_data: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let nonce = value_data.get(0..12)?;
    let tag = value_data.get(12..28)?;
    let mut buffer = Zeroizing::new(value_data.get(28..)?.to_vec());
    let cipher = <BitLockerCcm as KeyInit>::new(GenericArray::from_slice(key));
    cipher
        .decrypt_in_place_detached(
            GenericArray::from_slice(nonce),
            &[],
            buffer.as_mut_slice(),
            GenericArray::from_slice(tag),
        )
        .ok()?;
    Some(buffer)
}

#[cfg(test)]
#[path = "../tests/unit/kdf.rs"]
mod tests;
