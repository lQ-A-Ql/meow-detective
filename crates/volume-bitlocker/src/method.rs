//! BitLocker volume encryption methods.
//!
//! An encryption method describes *how the volume is encrypted*. It is
//! independent of how the volume master key is protected — see
//! [`crate::ProtectorKind`] for that axis.

/// A BitLocker volume encryption method, as recorded in the FVE metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncryptionMethod {
    /// `0x8000` — AES-128-CBC with the Elephant Diffuser.
    Aes128CbcDiffuser,
    /// `0x8001` — AES-256-CBC with the Elephant Diffuser.
    ///
    /// Recognized but not decryptable in v1: there is no validation oracle for
    /// it, and shipping an unvalidated cipher path on an evidence reader would
    /// produce plausible-looking wrong plaintext.
    Aes256CbcDiffuser,
    /// `0x8002` — AES-128-CBC, no diffuser.
    Aes128Cbc,
    /// `0x8003` — AES-256-CBC, no diffuser.
    Aes256Cbc,
    /// `0x8004` — XTS-AES-128.
    XtsAes128,
    /// `0x8005` — XTS-AES-256.
    XtsAes256,
    /// A method code this build does not recognize.
    ///
    /// Carried through rather than rejected at parse time so that the protector
    /// inventory of an unknown-cipher volume is still reportable.
    Unknown(u16),
}

impl EncryptionMethod {
    /// Classifies an FVE metadata encryption-method code.
    #[must_use]
    pub fn from_code(code: u16) -> Self {
        match code {
            0x8000 => Self::Aes128CbcDiffuser,
            0x8001 => Self::Aes256CbcDiffuser,
            0x8002 => Self::Aes128Cbc,
            0x8003 => Self::Aes256Cbc,
            0x8004 => Self::XtsAes128,
            0x8005 => Self::XtsAes256,
            other => Self::Unknown(other),
        }
    }

    /// Returns the FVE metadata code for this method.
    #[must_use]
    pub fn code(self) -> u16 {
        match self {
            Self::Aes128CbcDiffuser => 0x8000,
            Self::Aes256CbcDiffuser => 0x8001,
            Self::Aes128Cbc => 0x8002,
            Self::Aes256Cbc => 0x8003,
            Self::XtsAes128 => 0x8004,
            Self::XtsAes256 => 0x8005,
            Self::Unknown(code) => code,
        }
    }

    /// Whether v1 can decrypt a volume using this method.
    ///
    /// `0x8001` is deliberately excluded: see [`Self::Aes256CbcDiffuser`].
    #[must_use]
    pub fn is_decryptable(self) -> bool {
        matches!(
            self,
            Self::Aes128CbcDiffuser
                | Self::Aes128Cbc
                | Self::Aes256Cbc
                | Self::XtsAes128
                | Self::XtsAes256
        )
    }

    /// Length in bytes of the FVEK this method uses.
    ///
    /// The XTS methods carry two AES keys concatenated, so `0x8004` needs 32
    /// bytes for two AES-128 keys and `0x8005` needs 64 for two AES-256 keys.
    /// Reading the wrong length yields a key that decrypts to plausible garbage
    /// rather than an error, so this mapping is load-bearing.
    #[must_use]
    pub fn fvek_len(self) -> Option<usize> {
        match self {
            Self::Aes128CbcDiffuser | Self::Aes128Cbc => Some(16),
            Self::Aes256Cbc | Self::XtsAes128 => Some(32),
            Self::XtsAes256 => Some(64),
            // 0x8001 would be 32 bytes, but reporting a length would imply a
            // decrypt path exists for it.
            Self::Aes256CbcDiffuser | Self::Unknown(_) => None,
        }
    }

    /// Whether this method uses a separate 16-byte diffuser tweak key.
    ///
    /// Only the Elephant Diffuser methods do. The XTS methods have a tweak in the
    /// cipher sense, but it comes from the second half of the FVEK rather than a
    /// distinct metadata field.
    #[must_use]
    pub fn uses_diffuser_tweak(self) -> bool {
        matches!(self, Self::Aes128CbcDiffuser | Self::Aes256CbcDiffuser)
    }

    /// A stable label for display and reporting.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Aes128CbcDiffuser => "AES-128-CBC + Elephant Diffuser",
            Self::Aes256CbcDiffuser => "AES-256-CBC + Elephant Diffuser",
            Self::Aes128Cbc => "AES-128-CBC",
            Self::Aes256Cbc => "AES-256-CBC",
            Self::XtsAes128 => "XTS-AES-128",
            Self::XtsAes256 => "XTS-AES-256",
            Self::Unknown(_) => "unknown encryption method",
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/method.rs"]
mod tests;
