//! Development key-injection channel (`MEOW_WECHAT_KEYS`).
//!
//! When an encrypted WCDB/SQLCipher database is encountered and the
//! `MEOW_WECHAT_KEYS` environment variable points to a JSON file of the
//! form `{"<fileEntryId>": "<hex-key>"}`. Full logical paths and database
//! basenames remain accepted for compatibility with older single-source
//! key files and offline tooling.
//! the matching key decrypts the database in memory via `sqlcipher4` and
//! the content parsers run on the plaintext. Without the variable, without
//! a matching entry, or on any decrypt failure, extraction falls back to
//! the v1 inventory behavior plus a warning.
//!
//! Keys are secrets: they live in `Zeroizing` buffers, are dropped after
//! database/WAL reconstruction, and never enter the payload, warnings, or
//! logs. This is a development channel for validated offline workflows
//! (keys recovered from memory dumps via `keyscan`), not a production
//! configuration surface.

use serde_json::Value;
use zeroize::Zeroizing;

use crate::sqlcipher4;

/// Environment variable naming the JSON key file.
pub const KEYS_ENV: &str = "MEOW_WECHAT_KEYS";
const IMAGE_KEY_ENTRY: &str = "__wechat_image_key_v2";
const IMAGE_XOR_KEY_ENTRY: &str = "__wechat_image_xor_key_v2";

pub struct ImageMaterial {
    pub aes_key: Zeroizing<[u8; 16]>,
    pub xor_key: u8,
}

/// Outcome of attempting the injection channel for one database.
pub enum Injected {
    /// Channel inactive (env var unset): stay silent, plain v1 behavior.
    Inactive,
    /// Decrypted plaintext database bytes plus the key retained only long
    /// enough to authenticate and decrypt any supplied WAL frames.
    Decrypted {
        plain: Vec<u8>,
        key: Zeroizing<[u8; 32]>,
    },
    /// Channel active but could not produce plaintext; the string is a
    /// key-free reason suitable for a payload warning.
    Failed(String),
}

/// Try to decrypt `data` (an encrypted database body) using the injected
/// key for `db_name`, if the channel is active.
pub fn try_decrypt(file_id: &str, logical_path: &str, db_name: &str, data: &[u8]) -> Injected {
    let Some(path) = std::env::var_os(KEYS_ENV) else {
        return Injected::Inactive;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => return Injected::Failed(format!("密钥文件不可读：{error}")),
    };
    let map: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => return Injected::Failed(format!("密钥文件不是合法 JSON：{error}")),
    };
    let hex = map
        .as_object()
        .and_then(|object| find_key_entry(object, file_id, logical_path, db_name))
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(hex) = hex else {
        return Injected::Failed(format!("密钥文件中没有 {db_name} 的条目"));
    };
    let hex = Zeroizing::new(hex);
    let key = match parse_key(&hex) {
        Ok(key) => key,
        Err(reason) => return Injected::Failed(reason),
    };
    match sqlcipher4::decrypt_database(&key, data) {
        Ok(plain) => Injected::Decrypted { plain, key },
        Err(reason) => Injected::Failed(format!("解密失败：{reason}")),
    }
}

pub fn image_material() -> Option<ImageMaterial> {
    let path = std::env::var_os(KEYS_ENV)?;
    let text = Zeroizing::new(std::fs::read_to_string(path).ok()?);
    let map: Value = serde_json::from_str(&text).ok()?;
    let object = map.as_object()?;
    let key_hex = Zeroizing::new(object.get(IMAGE_KEY_ENTRY)?.as_str()?.to_string());
    let xor_hex = object.get(IMAGE_XOR_KEY_ENTRY)?.as_str()?;
    Some(ImageMaterial {
        aes_key: parse_image_key(&key_hex)?,
        xor_key: u8::from_str_radix(xor_hex, 16).ok()?,
    })
}

/// Prefer the source-unique file id. Path and basename lookup remain as
/// compatibility fallbacks for existing key files and offline tooling.
fn find_key_entry<'a>(
    object: &'a serde_json::Map<String, Value>,
    file_id: &str,
    logical_path: &str,
    db_name: &str,
) -> Option<&'a Value> {
    [file_id, logical_path, db_name]
        .into_iter()
        .find_map(|wanted| {
            object.get(wanted).or_else(|| {
                object
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
                    .map(|(_, value)| value)
            })
        })
}

/// Hex-decode a 32-byte key; the error text never echoes the key material.
fn parse_key(hex: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let bytes = hex.trim();
    if bytes.len() != 64 || !bytes.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("密钥不是 64 位十六进制字符串".to_string());
    }
    let mut key = [0u8; 32];
    for (index, pair) in bytes.as_bytes().chunks(2).enumerate() {
        key[index] = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("zz"), 16)
            .map_err(|_| "密钥不是 64 位十六进制字符串".to_string())?;
    }
    Ok(Zeroizing::new(key))
}

fn parse_image_key(hex: &str) -> Option<Zeroizing<[u8; 16]>> {
    let bytes = hex.trim();
    if bytes.len() != 32 || !bytes.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut key = [0u8; 16];
    for (index, pair) in bytes.as_bytes().chunks_exact(2).enumerate() {
        key[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(Zeroizing::new(key))
}
