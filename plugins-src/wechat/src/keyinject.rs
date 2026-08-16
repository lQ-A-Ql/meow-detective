//! Development key-injection channel (`MEOW_WECHAT_KEYS`).
//!
//! When an encrypted WCDB/SQLCipher database is encountered and the
//! `MEOW_WECHAT_KEYS` environment variable points to a JSON file of the
//! form `{"<dbName>": "<hex-key>"}` (e.g. `{"message_0.db": "b0fb…"}`),
//! the matching key decrypts the database in memory via `sqlcipher4` and
//! the content parsers run on the plaintext. Without the variable, without
//! a matching entry, or on any decrypt failure, extraction falls back to
//! the v1 inventory behavior plus a warning.
//!
//! Keys are secrets: they live in `Zeroizing` buffers, are dropped
//! immediately after decryption, and never enter the payload, warnings, or
//! logs. This is a development channel for validated offline workflows
//! (keys recovered from memory dumps via `keyscan`), not a production
//! configuration surface.

use serde_json::Value;
use zeroize::Zeroizing;

use crate::sqlcipher4;

/// Environment variable naming the JSON key file.
pub const KEYS_ENV: &str = "MEOW_WECHAT_KEYS";

/// Outcome of attempting the injection channel for one database.
pub enum Injected {
    /// Channel inactive (env var unset): stay silent, plain v1 behavior.
    Inactive,
    /// Decrypted plaintext database bytes.
    Decrypted(Vec<u8>),
    /// Channel active but could not produce plaintext; the string is a
    /// key-free reason suitable for a payload warning.
    Failed(String),
}

/// Try to decrypt `data` (an encrypted database body) using the injected
/// key for `db_name`, if the channel is active.
pub fn try_decrypt(db_name: &str, data: &[u8]) -> Injected {
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
        .and_then(|object| {
            object.get(db_name).or_else(|| {
                object
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(db_name))
                    .map(|(_, v)| v)
            })
        })
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
        Ok(plain) => Injected::Decrypted(plain),
        Err(reason) => Injected::Failed(format!("解密失败：{reason}")),
    }
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
