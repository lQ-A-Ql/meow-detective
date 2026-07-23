//! SECURITY hive traversal for offline LSA secret decryption.

use zeroize::Zeroizing;

use super::super::RegistryHiveReader;
use super::keys::{decrypt_lsa_key, decrypt_lsa_secret};
use crate::registry::RegistryError;

/// One decrypted LSA secret (current value only).
#[derive(Debug)]
pub struct LsaDecryptedSecret {
    pub name: String,
    pub secret: Zeroizing<Vec<u8>>,
}

/// Result of decrypting an offline SECURITY hive: the LSA key plus every
/// current secret that could be decrypted.
#[derive(Debug)]
pub struct LsaDecryptedSecrets {
    pub lsa_key: Zeroizing<Vec<u8>>,
    pub secrets: Vec<LsaDecryptedSecret>,
}

impl LsaDecryptedSecrets {
    /// Borrow a decrypted secret by name (case-insensitive).
    pub fn secret(&self, name: &str) -> Option<&[u8]> {
        self.secrets
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .map(|entry| entry.secret.as_slice())
    }
}

/// Derive the LSA key and decrypt all current `Policy\Secrets\*` values from
/// an offline SECURITY hive. Only the NT6 (Vista+) format is supported.
pub fn decrypt_lsa_secrets(
    security_hive: &[u8],
    boot_key: &[u8; 16],
) -> Result<LsaDecryptedSecrets, RegistryError> {
    let hive = RegistryHiveReader::new(security_hive)?;
    let pol_ek_list = read_default_at(&hive, &["Policy", "PolEKList"])?.ok_or_else(|| {
        if has_key(&hive, &["Policy", "PolSecretEncryptionKey"]) {
            RegistryError::UnsupportedCipher("legacy (pre-Vista) LSA secret encryption".to_string())
        } else {
            RegistryError::missing_key(r"Policy\PolEKList")
        }
    })?;
    let lsa_key = decrypt_lsa_key(boot_key, &pol_ek_list)?;

    let mut secrets = Vec::new();
    let Some(secrets_key) = hive.navigate_to(&["Policy", "Secrets"])? else {
        return Ok(LsaDecryptedSecrets { lsa_key, secrets });
    };
    for (name, secret_nk) in hive.read_subkeys_from_nk(&secrets_key)? {
        let Some(encrypted) = read_secret_value(&hive, &secret_nk, "CurrVal")? else {
            continue;
        };
        if encrypted.is_empty() {
            continue;
        }
        if let Ok(secret) = decrypt_lsa_secret(&lsa_key, &encrypted) {
            secrets.push(LsaDecryptedSecret { name, secret });
        }
    }
    Ok(LsaDecryptedSecrets { lsa_key, secrets })
}

/// Read a secret's current value, tolerating both on-disk layouts: a `CurrVal`
/// subkey with a default value, or a direct `CurrVal` value on the secret key.
fn read_secret_value(
    hive: &RegistryHiveReader<'_>,
    secret_nk: &super::super::NkRecord,
    value: &str,
) -> Result<Option<Vec<u8>>, RegistryError> {
    if let Some(bytes) = hive.read_raw_value_bytes(secret_nk, value)? {
        return Ok(Some(bytes));
    }
    for (name, child) in hive.read_subkeys_from_nk(secret_nk)? {
        if name.eq_ignore_ascii_case(value) {
            return hive
                .read_raw_value_bytes(&child, "")
                .map_err(RegistryError::from);
        }
    }
    Ok(None)
}

/// Read the default value of the key at `path`.
fn read_default_at(
    hive: &RegistryHiveReader<'_>,
    path: &[&str],
) -> Result<Option<Vec<u8>>, RegistryError> {
    let Some(nk) = hive.navigate_to(path)? else {
        return Ok(None);
    };
    hive.read_raw_value_bytes(&nk, "")
        .map_err(RegistryError::from)
}

fn has_key(hive: &RegistryHiveReader<'_>, path: &[&str]) -> bool {
    hive.navigate_to(path).ok().flatten().is_some()
}
