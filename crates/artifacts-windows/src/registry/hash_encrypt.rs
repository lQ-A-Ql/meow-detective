//! Write-side counterpart of `hash_decrypt`: re-encrypts a 16-byte SAM hash
//! into an existing V-value blob, preserving the blob's revision, header, and
//! salt. Used by the SAM bypass editor to write the canonical empty hashes.

use aes::cipher::{BlockEncrypt, KeyInit};
use cipher::generic_array::GenericArray;
use des::Des;

use super::hash_decrypt::{md5, rc4_crypt, rid_to_des_keys};

/// The canonical empty LM/NT hash bytes, for [`encrypt_hash_into_blob`].
pub(crate) const EMPTY_LM_HASH: [u8; 16] = [
    0xaa, 0xd3, 0xb4, 0x35, 0xb5, 0x14, 0x04, 0xee, 0xaa, 0xd3, 0xb4, 0x35, 0xb5, 0x14, 0x04, 0xee,
];
pub(crate) const EMPTY_NT_HASH: [u8; 16] = [
    0x31, 0xd6, 0xcf, 0xe0, 0xd1, 0x6a, 0xe9, 0x31, 0xb7, 0x3c, 0x59, 0xd7, 0xe0, 0xc0, 0x89, 0xc0,
];
pub(crate) const NTPASSWORD_CONSTANT: &[u8] = b"NTPASSWORD\0";
pub(crate) const LMPASSWORD_CONSTANT: &[u8] = b"LMPASSWORD\0";

/// Inverse of the hash decryption path: encrypt `plaintext_hash` with the
/// account's RID keys and wrap it into the blob's existing encryption scheme.
pub(crate) fn encrypt_hash_into_blob(
    hashed_boot_key: &[u8; 32],
    rid: u32,
    plaintext_hash: [u8; 16],
    constant: &[u8],
    existing_blob: &[u8],
) -> Option<Vec<u8>> {
    let (k1, k2) = rid_to_des_keys(rid);
    let intermediate = des_encrypt_16(&k1, &k2, &plaintext_hash);
    let revision = u16::from_le_bytes(existing_blob.get(2..4)?.try_into().ok()?);
    match revision {
        1 if existing_blob.len() == 20 => {
            let mut rc4_key_input = Vec::with_capacity(16 + 4 + constant.len());
            rc4_key_input.extend_from_slice(&hashed_boot_key[..16]);
            rc4_key_input.extend_from_slice(&rid.to_le_bytes());
            rc4_key_input.extend_from_slice(constant);
            let rc4_key = md5(&rc4_key_input);
            let mut out = existing_blob.to_vec();
            out[4..20].copy_from_slice(&rc4_crypt(&rc4_key, &intermediate));
            Some(out)
        }
        2 if existing_blob.len() == 40 => {
            // Single-block payload: salt + 16 bytes, no padding.
            let salt: [u8; 16] = existing_blob[8..24].try_into().ok()?;
            let data = aes128_cbc_encrypt(&hashed_boot_key[..16], &salt, &intermediate)?;
            let mut out = existing_blob.to_vec();
            out[24..40].copy_from_slice(&data);
            Some(out)
        }
        2 if existing_blob.len() == 56 => {
            // Real Windows 10+ blobs pad the 16-byte payload to two blocks
            // with PKCS#7 (0x10 x16).
            let salt: [u8; 16] = existing_blob[8..24].try_into().ok()?;
            let mut padded = [0x10u8; 32];
            padded[..16].copy_from_slice(&intermediate);
            let data = aes128_cbc_encrypt(&hashed_boot_key[..16], &salt, &padded)?;
            let mut out = existing_blob.to_vec();
            out[24..56].copy_from_slice(&data);
            Some(out)
        }
        _ => None,
    }
}

fn des_encrypt_16(key1: &[u8; 8], key2: &[u8; 8], plaintext: &[u8; 16]) -> [u8; 16] {
    let mut ciphertext = [0u8; 16];
    ciphertext[..8].copy_from_slice(&des_encrypt_block(key1, plaintext[..8].try_into().unwrap()));
    ciphertext[8..].copy_from_slice(&des_encrypt_block(
        key2,
        plaintext[8..16].try_into().unwrap(),
    ));
    ciphertext
}

fn des_encrypt_block(key: &[u8; 8], plaintext: &[u8; 8]) -> [u8; 8] {
    let cipher = Des::new_from_slice(key).expect("DES key length is 8 bytes");
    let mut block = GenericArray::clone_from_slice(plaintext);
    cipher.encrypt_block(&mut block);
    let mut out = [0u8; 8];
    out.copy_from_slice(&block);
    out
}

fn aes128_cbc_encrypt(key: &[u8], iv: &[u8; 16], plaintext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 16 || plaintext.is_empty() || !plaintext.len().is_multiple_of(16) {
        return None;
    }
    let cipher = aes::Aes128::new_from_slice(key).ok()?;
    let mut prev: [u8; 16] = *iv;
    let mut ciphertext = vec![0u8; plaintext.len()];
    for (i, chunk) in plaintext.chunks_exact(16).enumerate() {
        let mut block = GenericArray::clone_from_slice(chunk);
        for j in 0..16 {
            block[j] ^= prev[j];
        }
        cipher.encrypt_block(&mut block);
        ciphertext[i * 16..(i + 1) * 16].copy_from_slice(&block);
        prev.copy_from_slice(&block);
    }
    Some(ciphertext)
}
