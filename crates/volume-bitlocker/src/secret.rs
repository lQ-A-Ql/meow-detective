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
    pub fn new(fvek: Vec<u8>, tweak: Option<Vec<u8>>) -> Self {
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

#[cfg(test)]
#[path = "../tests/unit/secret.rs"]
mod tests;
