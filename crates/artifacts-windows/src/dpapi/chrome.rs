use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use zeroize::{Zeroize, Zeroizing};

use super::blob::parse_dpapi_blob;
use super::error::DpapiError;
use super::master_key::DecryptedMasterKey;

const V10_PREFIX: &[u8] = b"v10";
const V11_PREFIX: &[u8] = b"v11";
const V20_PREFIX: &[u8] = b"v20";
const NONCE_LEN: usize = 12;
const GCM_TAG_LEN: usize = 16;
const COOKIE_HOST_DIGEST_LEN: usize = 32;
const MAX_PREVIEW_CHARS: usize = 512;

/// Decryption state exposed to artifact projection without exposing key bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserDecryption {
    Plaintext,
    Decrypted,
    Encrypted,
    Unsupported,
    Failed,
    Unavailable,
}

impl BrowserDecryption {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Decrypted => "decrypted",
            Self::Encrypted => "encrypted",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Identifies which Chromium encoding was observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromiumValueKind {
    Plaintext,
    V10,
    V11,
    V20,
    LegacyDpapi,
    Unknown,
}

/// A read-only keyring of recovered DPAPI master keys and a profile's Local
/// State key. Key material is zeroized when this object is dropped.
#[derive(Debug, Clone)]
pub struct ChromiumDecryptor {
    master_keys: HashMap<String, Zeroizing<[u8; 64]>>,
    local_state_key: Zeroizing<[u8; 32]>,
}

impl ChromiumDecryptor {
    /// Build a decryptor from a Local State JSON document and recovered keys.
    pub fn from_local_state(
        local_state: &[u8],
        master_keys: &[DecryptedMasterKey],
    ) -> Result<Self, DpapiError> {
        let mut keyring = HashMap::new();
        for master_key in master_keys {
            keyring.insert(
                normalize_guid(&master_key.guid),
                Zeroizing::new(master_key.key),
            );
        }
        let local_state_key = decrypt_local_state_key(local_state, &keyring)?;
        Ok(Self {
            master_keys: keyring,
            local_state_key: Zeroizing::new(local_state_key),
        })
    }

    pub fn decrypt_value(
        &self,
        encrypted: &[u8],
        host_key: Option<&str>,
    ) -> (BrowserDecryption, Option<String>, Option<String>) {
        decrypt_chromium_value(
            encrypted,
            host_key,
            &self.local_state_key,
            &self.master_keys,
        )
    }
}

impl Drop for ChromiumDecryptor {
    fn drop(&mut self) {
        for key in self.master_keys.values_mut() {
            key.as_mut().zeroize();
        }
        self.local_state_key.as_mut().zeroize();
    }
}

/// Unwrap `os_crypt.encrypted_key` from a Chromium Local State document.
fn decrypt_local_state_key(
    local_state: &[u8],
    master_keys: &HashMap<String, Zeroizing<[u8; 64]>>,
) -> Result<[u8; 32], DpapiError> {
    let root: Value =
        serde_json::from_slice(local_state).map_err(|_| DpapiError::InvalidLocalStateKey)?;
    let encoded = root
        .get("os_crypt")
        .and_then(|value| value.get("encrypted_key"))
        .and_then(Value::as_str)
        .ok_or(DpapiError::InvalidLocalStateKey)?;
    let wrapped = STANDARD
        .decode(encoded)
        .map_err(|_| DpapiError::InvalidLocalStateKey)?;
    if wrapped.len() <= 5 || &wrapped[..5] != b"DPAPI" {
        return Err(DpapiError::InvalidLocalStateKey);
    }
    let blob = parse_dpapi_blob(&wrapped[5..])?;
    let master_key = master_keys
        .get(&normalize_guid(&blob.master_key_guid))
        .ok_or(DpapiError::NoMatchingMasterKey)?;
    let plaintext = blob.decrypt(master_key.as_ref())?;
    if plaintext.len() != 32 {
        return Err(DpapiError::InvalidLocalStateKey);
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&plaintext);
    Ok(key)
}

/// Decrypt a Chromium cookie/password value and return a bounded display
/// preview plus an auditable status/detail pair.
pub(crate) fn decrypt_chromium_value(
    encrypted: &[u8],
    host_key: Option<&str>,
    local_state_key: &[u8; 32],
    master_keys: &HashMap<String, Zeroizing<[u8; 64]>>,
) -> (BrowserDecryption, Option<String>, Option<String>) {
    if encrypted.is_empty() {
        return (BrowserDecryption::Plaintext, None, None);
    }
    let kind = value_kind(encrypted);
    match kind {
        ChromiumValueKind::V20 => (
            BrowserDecryption::Unsupported,
            Some("Chromium App-Bound Encryption v20 is not supported offline".to_string()),
            None,
        ),
        ChromiumValueKind::V10 | ChromiumValueKind::V11 => {
            match decrypt_gcm_value(encrypted, local_state_key, host_key) {
                Ok(bytes) => (
                    BrowserDecryption::Decrypted,
                    Some(preview_bytes(&bytes)),
                    None,
                ),
                Err(_) => (
                    BrowserDecryption::Failed,
                    Some("Chromium AES-GCM authentication failed".to_string()),
                    None,
                ),
            }
        }
        ChromiumValueKind::LegacyDpapi => {
            let blob = match parse_dpapi_blob(encrypted) {
                Ok(blob) => blob,
                Err(_) => {
                    return (
                        BrowserDecryption::Failed,
                        Some("legacy DPAPI value is malformed".to_string()),
                        None,
                    )
                }
            };
            let Some(master_key) = master_keys.get(&normalize_guid(&blob.master_key_guid)) else {
                return (
                    BrowserDecryption::Unavailable,
                    Some("matching DPAPI master key is unavailable".to_string()),
                    None,
                );
            };
            match blob.decrypt(master_key.as_ref()) {
                Ok(bytes) => (
                    BrowserDecryption::Decrypted,
                    Some(preview_bytes(&bytes)),
                    None,
                ),
                Err(_) => (
                    BrowserDecryption::Failed,
                    Some("legacy DPAPI value integrity verification failed".to_string()),
                    None,
                ),
            }
        }
        ChromiumValueKind::Unknown | ChromiumValueKind::Plaintext => (
            BrowserDecryption::Encrypted,
            Some(format!("[encrypted {} bytes]", encrypted.len())),
            None,
        ),
    }
}

fn decrypt_gcm_value(
    encrypted: &[u8],
    key: &[u8; 32],
    host_key: Option<&str>,
) -> Result<Vec<u8>, DpapiError> {
    if encrypted.len() < 3 + NONCE_LEN + GCM_TAG_LEN {
        return Err(DpapiError::InvalidChromiumValue);
    }
    let nonce = Nonce::from_slice(&encrypted[3..3 + NONCE_LEN]);
    let ciphertext = &encrypted[3 + NONCE_LEN..];
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| DpapiError::InvalidKeyLength)?;
    let mut plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| DpapiError::DecryptionFailed)?;
    if let Some(host_key) = host_key {
        let digest = Sha256::digest(host_key.as_bytes());
        if plaintext.len() >= COOKIE_HOST_DIGEST_LEN
            && plaintext[..COOKIE_HOST_DIGEST_LEN] == digest[..]
        {
            plaintext.drain(..COOKIE_HOST_DIGEST_LEN);
        }
    }
    Ok(plaintext)
}

fn value_kind(value: &[u8]) -> ChromiumValueKind {
    if value.starts_with(V10_PREFIX) {
        ChromiumValueKind::V10
    } else if value.starts_with(V11_PREFIX) {
        ChromiumValueKind::V11
    } else if value.starts_with(V20_PREFIX) {
        ChromiumValueKind::V20
    } else if value.first().copied() == Some(1) {
        ChromiumValueKind::LegacyDpapi
    } else {
        ChromiumValueKind::Unknown
    }
}

fn preview_bytes(bytes: &[u8]) -> String {
    let trimmed = bytes
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed
        .iter()
        .all(|byte| (0x20..=0x7e).contains(byte) || *byte >= 0x80)
    {
        return String::from_utf8_lossy(&trimmed)
            .chars()
            .take(MAX_PREVIEW_CHARS)
            .collect();
    }
    let visible = trimmed.len().min(64);
    format!(
        "[binary {} bytes: {}]",
        bytes.len(),
        hex::encode(&trimmed[..visible])
    )
}

fn normalize_guid(guid: &str) -> String {
    guid.trim_matches('{')
        .trim_matches('}')
        .to_ascii_lowercase()
}
