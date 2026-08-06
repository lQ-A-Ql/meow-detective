//! SAM password-hash decryption.
//!
//! Implements the pypykatz/ForensicsTool-style decryption pipeline:
//!
//! 1. Extract the BootKey (SysKey) from the SYSTEM hive.
//! 2. Use the BootKey to decrypt the `SAM\Domains\Account\F` key into the
//!    hashed boot key (`hbootkey`).
//! 3. For each local user, decrypt the LM/NT hashes stored in the user's `V`
//!    value using the hashed boot key and the user's RID.
//!
//! This module intentionally keeps all crypto primitives explicit and
//! dependency-only for the block/stream ciphers and MD5, so the registry
//! format logic stays readable and testable.

use aes::cipher::{BlockDecrypt, KeyInit};
use cipher::generic_array::GenericArray;
use des::Des;
use md5::{Digest, Md5};

// ── Constants ────────────────────────────────────────────────────────────────

const QWERTY: &[u8] = b"!@#$%^&*()qwertyUIOPAzxcvbnmQQQQQQQQQQQQ)(*@&%\0";
const DIGITS: &[u8] = b"0123456789012345678901234567890123456789\0";

const NTPASSWORD: &[u8] = b"NTPASSWORD\0";
const LMPASSWORD: &[u8] = b"LMPASSWORD\0";

pub(crate) const LM_HASH_EMPTY: &str = "aad3b435b51404eeaad3b435b51404ee";
pub(crate) const NT_HASH_EMPTY: &str = "31d6cfe0d16ae931b73c59d7e0c089c0";

// ── Public types ─────────────────────────────────────────────────────────────

/// Decrypted LM and NT hashes for a single SAM account.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SamHashes {
    /// Hex-encoded LM hash, or the canonical empty-LM value if no LM hash
    /// is stored.
    pub lm: String,
    /// Hex-encoded NT hash, or the canonical empty-NT value if no NT hash
    /// is stored.
    pub nt: String,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Derive the SAM hashed boot key from the SYSTEM BootKey and the raw
/// `SAM\Domains\Account\F` value.
///
/// Returns `None` when the domain key structure is unrecognized, the data is
/// too short, or the RC4 checksum / AES padding is invalid.
pub fn derive_hashed_boot_key(boot_key: [u8; 16], account_f: &[u8]) -> Option<[u8; 32]> {
    match parse_domain_key_data(account_f)? {
        DomainKeyData::Rc4 {
            salt,
            key,
            checksum,
        } => derive_hashed_boot_key_rc4(boot_key, salt, key, checksum),
        DomainKeyData::Aes { salt, data } => derive_hashed_boot_key_aes(boot_key, salt, &data),
    }
}

/// Decrypt the LM and NT hashes for a user given the hashed boot key, the
/// user's RID, and the raw `V` value bytes.
pub fn decrypt_user_hashes(
    hashed_boot_key: [u8; 32],
    rid: u32,
    user_v: &[u8],
) -> Option<SamHashes> {
    let hashes = parse_user_v_hashes(user_v)?;
    let lm = decrypt_hash(&hashed_boot_key, rid, hashes.lm.as_ref(), LMPASSWORD)
        .map(hex::encode)
        .unwrap_or_else(|| LM_HASH_EMPTY.to_string());
    let nt = decrypt_hash(&hashed_boot_key, rid, hashes.nt.as_ref(), NTPASSWORD)
        .map(hex::encode)
        .unwrap_or_else(|| NT_HASH_EMPTY.to_string());
    Some(SamHashes { lm, nt })
}

// ── Domain key parsing ───────────────────────────────────────────────────────

enum DomainKeyData {
    /// RC4-protected domain key (older Windows versions).
    Rc4 {
        salt: [u8; 16],
        key: [u8; 16],
        checksum: [u8; 16],
    },
    /// AES-CBC-protected domain key (Windows 10 1903+).
    Aes { salt: [u8; 16], data: Vec<u8> },
}

/// The encryption key data lives after a fixed-size header in
/// `SAM\Domains\Account\F`. The marker byte at this offset selects the
/// key format.
const DOMAIN_KEY_OFFSET: usize = 0x68;

fn parse_domain_key_data(data: &[u8]) -> Option<DomainKeyData> {
    if data.len() < DOMAIN_KEY_OFFSET + 4 {
        return None;
    }
    let marker = data[DOMAIN_KEY_OFFSET];
    match marker {
        1 => {
            // RC4 variant: Revision(4) + Length(4) + Salt(16) + Key(16) +
            // CheckSum(16) + Reserved(8) = 64 bytes total.
            if data.len() < DOMAIN_KEY_OFFSET + 64 {
                return None;
            }
            let salt = data[DOMAIN_KEY_OFFSET + 8..DOMAIN_KEY_OFFSET + 24]
                .try_into()
                .ok()?;
            let key = data[DOMAIN_KEY_OFFSET + 24..DOMAIN_KEY_OFFSET + 40]
                .try_into()
                .ok()?;
            let checksum = data[DOMAIN_KEY_OFFSET + 40..DOMAIN_KEY_OFFSET + 56]
                .try_into()
                .ok()?;
            Some(DomainKeyData::Rc4 {
                salt,
                key,
                checksum,
            })
        }
        2 => {
            // AES variant: Revision(4) + Length(4) + CheckSumLength(4) +
            // DataLength(4) + Salt(16) + Data(DataLength).
            if data.len() < DOMAIN_KEY_OFFSET + 24 {
                return None;
            }
            let data_length = u32::from_le_bytes(
                data[DOMAIN_KEY_OFFSET + 12..DOMAIN_KEY_OFFSET + 16]
                    .try_into()
                    .ok()?,
            ) as usize;
            let end = DOMAIN_KEY_OFFSET
                .checked_add(32)
                .and_then(|o| o.checked_add(data_length))?;
            if data.len() < end {
                return None;
            }
            let salt = data[DOMAIN_KEY_OFFSET + 16..DOMAIN_KEY_OFFSET + 32]
                .try_into()
                .ok()?;
            let enc_data = data[DOMAIN_KEY_OFFSET + 32..end].to_vec();
            Some(DomainKeyData::Aes {
                salt,
                data: enc_data,
            })
        }
        _ => None,
    }
}

fn derive_hashed_boot_key_rc4(
    boot_key: [u8; 16],
    salt: [u8; 16],
    key: [u8; 16],
    checksum: [u8; 16],
) -> Option<[u8; 32]> {
    let mut rc4_key_input = Vec::with_capacity(salt.len() + QWERTY.len() + 16 + DIGITS.len());
    rc4_key_input.extend_from_slice(&salt);
    rc4_key_input.extend_from_slice(QWERTY);
    rc4_key_input.extend_from_slice(&boot_key);
    rc4_key_input.extend_from_slice(DIGITS);
    let rc4_key = md5(&rc4_key_input);

    let mut encrypted = [0u8; 32];
    encrypted[..16].copy_from_slice(&key);
    encrypted[16..].copy_from_slice(&checksum);
    let decrypted = rc4_decrypt(&rc4_key, &encrypted)?;
    if decrypted.len() != 32 {
        return None;
    }

    let first16: [u8; 16] = decrypted[..16].try_into().ok()?;
    let mut check_input = Vec::with_capacity(16 + DIGITS.len() + 16 + QWERTY.len());
    check_input.extend_from_slice(&first16);
    check_input.extend_from_slice(DIGITS);
    check_input.extend_from_slice(&first16);
    check_input.extend_from_slice(QWERTY);
    let expected = md5(&check_input);

    if expected != decrypted[16..32] {
        return None;
    }

    let mut out = [0u8; 32];
    out[..16].copy_from_slice(&first16);
    out[16..].copy_from_slice(&decrypted[16..32]);
    Some(out)
}

fn derive_hashed_boot_key_aes(boot_key: [u8; 16], salt: [u8; 16], data: &[u8]) -> Option<[u8; 32]> {
    if data.is_empty() || !data.len().is_multiple_of(16) {
        return None;
    }
    let plaintext = aes128_cbc_decrypt(&boot_key, &salt, data)?;
    if plaintext.len() < 32 {
        return None;
    }
    plaintext[..32].try_into().ok()
}

// ── User V hash parsing ──────────────────────────────────────────────────────

const USER_V_HEADER_LEN: usize = 204;

enum EncryptedHash {
    /// RC4-protected hash: PekID(2) + Revision(2) + Hash(16).
    Rc4([u8; 16]),
    /// AES-CBC-protected hash: PekID(2) + Revision(2) + DataOffset(4) +
    /// Salt(16) + Hash(...).
    Aes { salt: [u8; 16], data: Vec<u8> },
}

struct UserHashes {
    lm: Option<EncryptedHash>,
    nt: Option<EncryptedHash>,
}

fn parse_user_v_hashes(v_data: &[u8]) -> Option<UserHashes> {
    if v_data.len() < USER_V_HEADER_LEN {
        return None;
    }

    let lm_offset = u32::from_le_bytes(v_data[0x9C..0xA0].try_into().ok()?) as usize;
    let lm_length = u32::from_le_bytes(v_data[0xA0..0xA4].try_into().ok()?) as usize;
    let nt_offset = u32::from_le_bytes(v_data[0xA8..0xAC].try_into().ok()?) as usize;
    let nt_length = u32::from_le_bytes(v_data[0xAC..0xB0].try_into().ok()?) as usize;

    let base = USER_V_HEADER_LEN;
    let lm = parse_encrypted_hash(v_data, base.checked_add(lm_offset)?, lm_length);
    let nt = parse_encrypted_hash(v_data, base.checked_add(nt_offset)?, nt_length);
    Some(UserHashes { lm, nt })
}

fn parse_encrypted_hash(data: &[u8], offset: usize, length: usize) -> Option<EncryptedHash> {
    if length == 0 {
        return None;
    }
    let end = offset.checked_add(length)?;
    if data.len() < end {
        return None;
    }
    let blob = &data[offset..end];
    let revision = u16::from_le_bytes(blob.get(2..4)?.try_into().ok()?);

    match revision {
        1 if blob.len() >= 20 => {
            let mut hash = [0u8; 16];
            hash.copy_from_slice(&blob[4..20]);
            Some(EncryptedHash::Rc4(hash))
        }
        2 if blob.len() >= 24 => {
            let encrypted = blob.get(24..)?;
            if encrypted.is_empty() || !encrypted.len().is_multiple_of(16) {
                return None;
            }
            let mut salt = [0u8; 16];
            salt.copy_from_slice(&blob[8..24]);
            Some(EncryptedHash::Aes {
                salt,
                data: encrypted.to_vec(),
            })
        }
        _ => None,
    }
}

// ── Per-hash decryption ──────────────────────────────────────────────────────

fn decrypt_hash(
    hashed_boot_key: &[u8; 32],
    rid: u32,
    enc: Option<&EncryptedHash>,
    constant: &[u8],
) -> Option<[u8; 16]> {
    let enc = enc?;
    let intermediate: [u8; 16] = match enc {
        EncryptedHash::Rc4(hash16) => {
            let mut rc4_key_input = Vec::with_capacity(16 + 4 + constant.len());
            rc4_key_input.extend_from_slice(&hashed_boot_key[..16]);
            rc4_key_input.extend_from_slice(&rid.to_le_bytes());
            rc4_key_input.extend_from_slice(constant);
            let rc4_key = md5(&rc4_key_input);
            let pt = rc4_decrypt(&rc4_key, hash16)?;
            pt.try_into().ok()?
        }
        EncryptedHash::Aes { salt, data } => {
            let pt = aes128_cbc_decrypt(&hashed_boot_key[..16], salt, data)?;
            if pt.len() < 16 {
                return None;
            }
            pt[..16].try_into().ok()?
        }
    };

    let (k1, k2) = rid_to_des_keys(rid);
    Some(des_decrypt_16(&k1, &k2, &intermediate))
}

// ── Crypto primitives ────────────────────────────────────────────────────────

pub(crate) fn md5(data: &[u8]) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn rc4_decrypt(key: &[u8; 16], ciphertext: &[u8]) -> Option<Vec<u8>> {
    Some(rc4_crypt(key, ciphertext))
}

/// Pure-Rust RC4 keystream generator (avoids a separate rc4 crate and keeps
/// the cipher dependency set on cipher 0.4 / aes / des only).
pub(crate) fn rc4_crypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }

    let mut i: u8 = 0;
    let mut j: u8 = 0;
    data.iter()
        .map(|b| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
            b ^ k
        })
        .collect()
}

pub(crate) fn rid_to_des_keys(rid: u32) -> ([u8; 8], [u8; 8]) {
    let rid_bytes = rid.to_le_bytes();
    let key1 = expand_des_key(&[
        rid_bytes[0],
        rid_bytes[1],
        rid_bytes[2],
        rid_bytes[3],
        rid_bytes[0],
        rid_bytes[1],
        rid_bytes[2],
    ]);
    let key2 = expand_des_key(&[
        rid_bytes[3],
        rid_bytes[0],
        rid_bytes[1],
        rid_bytes[2],
        rid_bytes[3],
        rid_bytes[0],
        rid_bytes[1],
    ]);
    (key1, key2)
}

fn expand_des_key(key: &[u8]) -> [u8; 8] {
    let mut k = [0u8; 7];
    let len = key.len().min(7);
    k[..len].copy_from_slice(&key[..len]);

    let mut result = [0u8; 8];
    result[0] = ((k[0] >> 1) & 0x7f) << 1;
    result[1] = ((k[0] & 0x01) << 6 | (k[1] >> 2) & 0x3f) << 1;
    result[2] = ((k[1] & 0x03) << 5 | (k[2] >> 3) & 0x1f) << 1;
    result[3] = ((k[2] & 0x07) << 4 | (k[3] >> 4) & 0x0f) << 1;
    result[4] = ((k[3] & 0x0f) << 3 | (k[4] >> 5) & 0x07) << 1;
    result[5] = ((k[4] & 0x1f) << 2 | (k[5] >> 6) & 0x03) << 1;
    result[6] = ((k[5] & 0x3f) << 1 | (k[6] >> 7) & 0x01) << 1;
    result[7] = (k[6] & 0x7f) << 1;
    result
}

fn des_decrypt_16(key1: &[u8; 8], key2: &[u8; 8], ciphertext: &[u8; 16]) -> [u8; 16] {
    let mut plaintext = [0u8; 16];
    plaintext[..8].copy_from_slice(&des_decrypt_block(
        key1,
        ciphertext[..8].try_into().unwrap(),
    ));
    plaintext[8..].copy_from_slice(&des_decrypt_block(
        key2,
        ciphertext[8..16].try_into().unwrap(),
    ));
    plaintext
}

fn des_decrypt_block(key: &[u8; 8], ciphertext: &[u8; 8]) -> [u8; 8] {
    let cipher = Des::new_from_slice(key).expect("DES key length is 8 bytes");
    let mut block = GenericArray::clone_from_slice(ciphertext);
    cipher.decrypt_block(&mut block);
    let mut out = [0u8; 8];
    out.copy_from_slice(&block);
    out
}

fn aes128_cbc_decrypt(key: &[u8], iv: &[u8; 16], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 16 || !ciphertext.len().is_multiple_of(16) {
        return None;
    }
    let cipher = aes::Aes128::new_from_slice(key).ok()?;
    let mut prev: [u8; 16] = *iv;
    let mut plaintext = vec![0u8; ciphertext.len()];

    for (i, chunk) in ciphertext.chunks_exact(16).enumerate() {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        for j in 0..16 {
            block[j] ^= prev[j];
        }
        plaintext[i * 16..(i + 1) * 16].copy_from_slice(&block);
        prev = chunk.try_into().ok()?;
    }
    Some(plaintext)
}

#[cfg(test)]
#[path = "../../tests/unit/registry/hash_decrypt.rs"]
mod tests;
