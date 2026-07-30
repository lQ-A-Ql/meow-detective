//! Secret-bearing types.
//!
//! Two rules govern everything in this module, and
//! `scripts/check-bitlocker-credential-guard.ps1` enforces both:
//!
//! 1. No secret type derives or implements `Debug`, `Clone`, or `Serialize`.
//!    `Debug` is the leak that matters most in practice — one `{:?}` in a log
//!    line or an error variant and the credential is on disk.
//! 2. Every secret zeroizes on drop.

use zeroize::Zeroizing;

use crate::{BitLockerError, Result};

/// The v1 envelope is fixed-header and contains at most a 64-byte FVEK and a
/// 16-byte diffuser tweak. Keeping the transport type bounded prevents a
/// corrupt platform credential from allocating or retaining arbitrary data.
pub(crate) const MAX_PERSISTED_KEY_BLOB_LEN: usize = 128;
pub(crate) const MIN_PERSISTED_KEY_BLOB_LEN: usize = 48;

/// An investigator-supplied credential: a password or a 48-digit recovery password.
///
/// Process-lifetime only. This type must never be persisted, serialized, put in
/// a job parameter, or included in an event. Only the *derived, verified* key
/// package ([`VolumeKeyPackage`]) may be stored.
///
/// Not `Clone`: every additional copy is another buffer to zeroize and another
/// chance to outlive the unlock attempt. Callers that need it twice should pass
/// a reference.
pub struct Passphrase {
    /// UTF-8 as entered. BitLocker's KDF re-encodes to UTF-16LE at derivation
    /// time; storing the UTF-16 form here would mean a second secret buffer.
    inner: Zeroizing<String>,
}

/// A structurally recovered 256-bit BitLocker volume master key.
///
/// This type is an internal handoff between the Windows memory parser and the
/// metadata authentication path. Construction does not by itself prove that the
/// bytes belong to a volume; only a successful reverse-datum CCM tag does that.
pub struct RecoveredVmk {
    inner: Zeroizing<[u8; 32]>,
}

impl RecoveredVmk {
    /// Moves an exact-size VMK into zeroizing storage.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self {
            inner: Zeroizing::new(bytes),
        }
    }

    pub(crate) fn expose_for_recovery(&self) -> &[u8; 32] {
        &self.inner
    }
}

/// A recovered 48-digit numerical recovery password.
///
/// The value is process-lifetime only. It must not be persisted, logged,
/// reported, or placed in application state. The single authorized reveal
/// boundary is the transient memory-unlock command response shown to the
/// investigator; it must never reach logs, reports, exports, or storage.
pub struct RecoveryPassword {
    inner: Zeroizing<String>,
}

impl RecoveryPassword {
    pub(crate) fn from_formatted(inner: String) -> Self {
        Self {
            inner: Zeroizing::new(inner),
        }
    }

    /// Exposes the password only to an explicitly authorized reveal boundary.
    #[must_use]
    pub fn expose_for_authorized_reveal(&self) -> &str {
        &self.inner
    }
}

impl Passphrase {
    /// Takes ownership of a credential string.
    ///
    /// The caller's original `String` is moved in, so no un-zeroized copy is
    /// left behind.
    #[must_use]
    pub fn new(secret: String) -> Self {
        Self {
            inner: Zeroizing::new(secret),
        }
    }

    /// Borrows the credential for key derivation.
    ///
    /// Intentionally the only accessor, and intentionally not named `as_str`:
    /// the name should make a reviewer ask why a secret is being read.
    #[must_use]
    pub fn expose_for_derivation(&self) -> &str {
        &self.inner
    }

    /// Whether the credential is empty.
    ///
    /// Length is the one property safe to expose: an empty credential is a
    /// caller bug worth rejecting before it reaches the KDF.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// A verified volume key package: the FVEK and, for XTS methods, the tweak key.
///
/// This is the only BitLocker secret that may be persisted, and only after it has
/// been verified against the volume. It is stored under the Credential Manager
/// target `Meow_Detective/BitLocker/v1/<metadataFingerprint>`.
///
/// Persisting this is a deliberate trade: it means the case can be reopened
/// without the password, and it also means the key package is permanent
/// decryption capability for that volume. Stage 4 owns the retention policy.
///
/// Not `Clone` and not `Debug`, for the same reasons as [`Passphrase`].
pub struct VolumeKeyPackage {
    fvek: Zeroizing<Vec<u8>>,
    tweak: Option<Zeroizing<Vec<u8>>>,
}

impl VolumeKeyPackage {
    /// Wraps verified key material.
    ///
    /// `tweak` is `Some` only for the XTS methods (`0x8004` / `0x8005`).
    #[must_use]
    pub(crate) fn new(fvek: Vec<u8>, tweak: Option<Vec<u8>>) -> Self {
        Self {
            fvek: Zeroizing::new(fvek),
            tweak: tweak.map(Zeroizing::new),
        }
    }

    /// Borrows the full-volume encryption key for cipher setup.
    #[must_use]
    pub fn expose_fvek(&self) -> &[u8] {
        &self.fvek
    }

    /// Borrows the XTS tweak key, if the method uses one.
    #[must_use]
    pub fn expose_tweak(&self) -> Option<&[u8]> {
        // Two derefs: Zeroizing<Vec<u8>> -> Vec<u8> -> [u8].
        self.tweak.as_ref().map(|tweak| tweak.as_slice())
    }
}

/// An opaque, bounded key-package envelope read from or written to secure
/// platform storage.
///
/// The bytes are deliberately inaccessible except to the storage adapter and
/// the crate's validated decoder. This type is not serializable and never
/// crosses the transport contract.
pub struct PersistedKeyBlob {
    inner: Zeroizing<Vec<u8>>,
}

impl PersistedKeyBlob {
    /// Accepts an untrusted platform credential blob after enforcing the
    /// allocation bound. Structural and volume-identity checks happen during
    /// restore.
    pub fn from_storage(bytes: Vec<u8>) -> Result<Self> {
        if !(MIN_PERSISTED_KEY_BLOB_LEN..=MAX_PERSISTED_KEY_BLOB_LEN).contains(&bytes.len()) {
            return Err(BitLockerError::PersistedKeyInvalid {
                reason: "blob length is outside the v1 envelope bounds",
            });
        }
        Ok(Self {
            inner: Zeroizing::new(bytes),
        })
    }

    pub(crate) fn encoded(bytes: Vec<u8>) -> Self {
        debug_assert!(bytes.len() <= MAX_PERSISTED_KEY_BLOB_LEN);
        Self {
            inner: Zeroizing::new(bytes),
        }
    }

    /// Borrows the opaque envelope for a secure-storage write.
    #[must_use]
    pub fn expose_for_storage(&self) -> &[u8] {
        &self.inner
    }
}

#[cfg(test)]
#[path = "../tests/unit/secret.rs"]
mod tests;
