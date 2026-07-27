//! BitLocker key protectors.
//!
//! A protector describes *how the volume master key is wrapped*. It is
//! independent of the volume cipher — see [`crate::EncryptionMethod`].
//!
//! The forensic value of this module is the inventory: an investigator needs to
//! know which protectors a locked volume carries, even when none of them can be
//! used to unlock it here.

/// A BitLocker key-protector type, as recorded in an FVE metadata entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtectorKind {
    /// `0x0000` — clear key. The volume master key is on disk unwrapped.
    ///
    /// Reported but never used to unlock in v1: decrypting with no credential
    /// should be an explicit, recorded investigator action, not an automatic
    /// fallback that silently produces plaintext.
    ClearKey,
    /// `0x0800` — 48-digit recovery password.
    RecoveryPassword,
    /// `0x2000` — user password.
    Password,
    /// TPM-sealed, with or without a PIN. Inventory only.
    Tpm,
    /// External startup key (a `.BEK` file). Inventory only.
    StartupKey,
    /// A protector code this build does not recognize. Inventory only.
    Unknown(u16),
}

impl ProtectorKind {
    /// Whether v1 can unlock a volume with this protector.
    #[must_use]
    pub fn is_unlockable(self) -> bool {
        matches!(self, Self::Password | Self::RecoveryPassword)
    }

    /// Whether this protector requires a credential from the investigator.
    ///
    /// Distinct from [`Self::is_unlockable`]: a clear key needs no credential
    /// yet is still not an unlock path in v1.
    #[must_use]
    pub fn requires_credential(self) -> bool {
        !matches!(self, Self::ClearKey)
    }

    /// A stable label for display and reporting.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ClearKey => "clear key",
            Self::RecoveryPassword => "recovery password",
            Self::Password => "password",
            Self::Tpm => "TPM",
            Self::StartupKey => "startup key",
            Self::Unknown(_) => "unknown protector",
        }
    }
}

/// Every protector found on a volume, whether or not it is usable here.
///
/// Deliberately not `Serialize`: the transport DTO is defined in
/// `crates/transport`, and this type must not become a second wire contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtectorInventory {
    protectors: Vec<ProtectorKind>,
}

impl ProtectorInventory {
    /// Builds an inventory from parsed metadata entries, preserving on-disk order.
    #[must_use]
    pub fn new(protectors: Vec<ProtectorKind>) -> Self {
        Self { protectors }
    }

    /// The protectors found, in on-disk order.
    #[must_use]
    pub fn protectors(&self) -> &[ProtectorKind] {
        &self.protectors
    }

    /// Whether any protector present can be unlocked by v1.
    #[must_use]
    pub fn has_unlockable_protector(&self) -> bool {
        self.protectors.iter().any(|p| p.is_unlockable())
    }

    /// Whether the inventory is empty.
    ///
    /// An empty inventory means the metadata parsed but carried no protector
    /// entries, which is a malformed-volume signal rather than "no protection".
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.protectors.is_empty()
    }
}

#[cfg(test)]
#[path = "../tests/unit/protector.rs"]
mod tests;
