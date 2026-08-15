//! Typed interpretations of decrypted LSA secrets used by offline DPAPI work.

use crate::registry::RegistryError;

const DPAPI_SYSTEM_SECRET_LEN: usize = 44;
const TBAL_MIN_LEN: usize = 0x58;
const TBAL_NT_HASH_RANGE: std::ops::Range<usize> = 0x10..0x20;
const TBAL_SHA1_RANGE: std::ops::Range<usize> = 0x30..0x44;
const TBAL_SHA1_DUP_RANGE: std::ops::Range<usize> = 0x44..0x58;

/// Machine/user prekeys recovered from the `DPAPI_SYSTEM` LSA secret.
///
/// Layout: `version(4) | machine_key(20) | user_key(20)`.
#[derive(Debug, Clone)]
pub struct DpapiSystemKeys {
    pub machine_key: [u8; 20],
    pub user_key: [u8; 20],
}

impl DpapiSystemKeys {
    /// Parse the decrypted `DPAPI_SYSTEM` secret payload.
    pub fn from_secret(raw: &[u8]) -> Result<Self, RegistryError> {
        if raw.len() != DPAPI_SYSTEM_SECRET_LEN {
            return Err(RegistryError::invalid_cell(format!(
                "DPAPI_SYSTEM secret must be {DPAPI_SYSTEM_SECRET_LEN} bytes, got {}",
                raw.len()
            )));
        }
        let mut machine_key = [0u8; 20];
        machine_key.copy_from_slice(&raw[4..24]);
        let mut user_key = [0u8; 20];
        user_key.copy_from_slice(&raw[24..44]);
        Ok(Self {
            machine_key,
            user_key,
        })
    }

    /// Both prekeys, machine first (matching common tool ordering).
    pub fn prekeys(&self) -> [&[u8]; 2] {
        [&self.machine_key, &self.user_key]
    }
}

/// Credential material recovered from an `M$_MSV1_0_TBAL_PRIMARY_*` LSA secret.
///
/// These secrets are provisioned by Automatic Restart Sign-On (TBAL) and hold
/// both the account NT hash and `SHA1(UTF-16LE(password))`, the latter being
/// the DPAPI prekey source for local accounts that the NT hash cannot serve.
#[derive(Debug, Clone)]
pub struct TbalSecret {
    pub nt_hash: [u8; 16],
    pub password_sha1: [u8; 20],
}

impl TbalSecret {
    /// Secret name prefix that marks a TBAL provisioning record.
    pub(crate) const NAME_PREFIX: &'static str = "M$_MSV1_0_TBAL_PRIMARY_";

    /// Parse a decrypted TBAL secret, requiring the duplicated password-SHA1
    /// fields to agree before trusting the offsets.
    pub fn from_secret(name: &str, raw: &[u8]) -> Option<Self> {
        if !name.starts_with(Self::NAME_PREFIX) || raw.len() < TBAL_MIN_LEN {
            return None;
        }
        let (sha_first, sha_second) = (&raw[TBAL_SHA1_RANGE], &raw[TBAL_SHA1_DUP_RANGE]);
        if sha_first != sha_second {
            return None;
        }
        let mut nt_hash = [0u8; 16];
        nt_hash.copy_from_slice(&raw[TBAL_NT_HASH_RANGE]);
        let mut password_sha1 = [0u8; 20];
        password_sha1.copy_from_slice(sha_first);
        Some(Self {
            nt_hash,
            password_sha1,
        })
    }
}
