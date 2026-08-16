//! SQLCipher 4 decryption for WeChat 4.x (WCDB) databases.
//!
//! Page layout (page size 4096): page 1 is `salt(16) | ciphertext | IV(16) |
//! HMAC(64)`; later pages are `ciphertext | IV(16) | HMAC(64)`. The raw
//! 32-byte key decrypts pages directly with AES-256-CBC (no padding); the
//! page HMAC uses SHA-512 over `ciphertext | IV | pgno_le` with a mac key of
//! `PBKDF2-HMAC-SHA512(key, salt ^ 0x3a, 2 iterations, 32 bytes)`.

use aes::cipher::{BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::Sha512;
use zeroize::Zeroizing;

pub const PAGE_SZ: usize = 4096;
pub const SALT_SZ: usize = 16;
const IV_SZ: usize = 16;
const HMAC_SZ: usize = 64;
const RESERVE_SZ: usize = IV_SZ + HMAC_SZ;
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

/// Verify a candidate key against page 1 of an encrypted database.
pub fn validate_page1(key: &[u8; 32], page1_with_salt: &[u8]) -> bool {
    if page1_with_salt.len() < PAGE_SZ {
        return false;
    }
    let salt = &page1_with_salt[..SALT_SZ];
    let page = &page1_with_salt[SALT_SZ..PAGE_SZ];
    let mac_key = mac_key_for(key, salt);
    let body_end = page.len() - RESERVE_SZ + IV_SZ; // ciphertext + IV
    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key[..]).expect("hmac key");
    mac.update(&page[..body_end]);
    mac.update(&1u32.to_le_bytes());
    let expected = mac.finalize().into_bytes();
    expected[..HMAC_SZ] == page[body_end..body_end + HMAC_SZ]
}

fn mac_key_for(key: &[u8; 32], salt: &[u8]) -> Zeroizing<[u8; 32]> {
    let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
    // SQLCipher 4: PBKDF2-HMAC-SHA512, 2 iterations, 32-byte output.
    Zeroizing::new(pbkdf2::pbkdf2_hmac_array::<Sha512, 32>(key, &mac_salt, 2))
}

/// Decrypt a whole database file into plaintext SQLite bytes.
///
/// The output starts with the plaintext SQLite header; per-page reserve bytes
/// (IV + HMAC) are preserved verbatim, matching how sqlcipher browser tools
/// re-emit decrypted WCDB files.
pub fn decrypt_database(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < PAGE_SZ || !data.len().is_multiple_of(PAGE_SZ) {
        return Err(format!(
            "encrypted database size {} is not a {PAGE_SZ}-page multiple",
            data.len()
        ));
    }
    if !validate_page1(key, &data[..PAGE_SZ]) {
        return Err("page-1 HMAC verification failed (wrong key?)".to_string());
    }

    let mut out = Vec::with_capacity(data.len() + SALT_SZ);
    out.extend_from_slice(SQLITE_HEADER);
    for (index, chunk) in data.chunks(PAGE_SZ).enumerate() {
        let page = if index == 0 { &chunk[SALT_SZ..] } else { chunk };
        let body_end = page.len() - RESERVE_SZ;
        let iv = &page[body_end..body_end + IV_SZ];
        let mut buf = page[..body_end].to_vec();
        Aes256CbcDec::new((&key[..]).into(), iv.into())
            .decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf)
            .map_err(|e| format!("AES-CBC decrypt failed on page {}: {e}", index + 1))?;
        out.extend_from_slice(&buf);
        out.extend_from_slice(&page[body_end..]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic encrypted page-1 buffer for the given key, mirroring
    /// the SQLCipher 4 layout (random salt/IV, zeroed content, valid HMAC).
    fn synthetic_page1(key: &[u8; 32]) -> Vec<u8> {
        let salt = [0x11u8; SALT_SZ];
        let mut page = vec![0u8; PAGE_SZ - SALT_SZ];
        let body_end = page.len() - RESERVE_SZ;
        // Deterministic "ciphertext": decryptable filler.
        for (i, b) in page[..body_end].iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        page[body_end..body_end + IV_SZ].copy_from_slice(&[0x22u8; IV_SZ]);
        let mac_key = mac_key_for(key, &salt);
        let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key[..]).expect("hmac key");
        mac.update(&page[..body_end + IV_SZ]);
        mac.update(&1u32.to_le_bytes());
        let tag = mac.finalize().into_bytes();
        page[body_end + IV_SZ..body_end + IV_SZ + HMAC_SZ].copy_from_slice(&tag[..HMAC_SZ]);
        let mut full = salt.to_vec();
        full.extend_from_slice(&page);
        full
    }

    #[test]
    fn validate_page1_accepts_matching_key() {
        let key = [0x42u8; 32];
        let page1 = synthetic_page1(&key);
        assert!(validate_page1(&key, &page1));
    }

    #[test]
    fn validate_page1_rejects_wrong_key() {
        let key = [0x42u8; 32];
        let wrong = [0x43u8; 32];
        let page1 = synthetic_page1(&key);
        assert!(!validate_page1(&wrong, &page1));
    }

    #[test]
    fn validate_page1_rejects_truncated_input() {
        let key = [0x42u8; 32];
        assert!(!validate_page1(&key, &[0u8; 100]));
    }
}
