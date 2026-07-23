//! NT6 LSA key derivation and per-secret decryption primitives.
//!
//! Layout reference (LSA_SECRET): `version(4) | enc_key_id(16) | enc_algo(4) |
//! flags(4) | data`, where `data[..32]` feeds the SHA-256 key expansion and
//! `data[32..]` is decrypted block-wise with AES-256 in ECB mode (final short
//! block zero-padded). The decrypted output is an LSA_SECRET_BLOB:
//! `length(4) | reserved(12) | secret[length]`.

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::registry::RegistryError;

const LSA_SECRET_HEADER_LEN: usize = 28;
const KEY_MATERIAL_LEN: usize = 32;
const SHA256_EXPANSION_ROUNDS: usize = 1000;
const BLOB_HEADER_LEN: usize = 16;
const LSA_KEY_OFFSET: usize = 52;
const LSA_KEY_LEN: usize = 32;

/// Derive the NT6 LSA key from the BootKey and the `Policy\PolEKList` default
/// value bytes.
pub(super) fn decrypt_lsa_key(
    boot_key: &[u8; 16],
    pol_ek_list: &[u8],
) -> Result<Zeroizing<Vec<u8>>, RegistryError> {
    let data = lsa_secret_data(pol_ek_list)?;
    let round_key = sha256_expand(boot_key, &data[..KEY_MATERIAL_LEN]);
    let decrypted = aes256_ecb_decrypt(&round_key, &data[KEY_MATERIAL_LEN..]);
    let blob = parse_secret_blob(&decrypted)?;
    if blob.len() < LSA_KEY_OFFSET + LSA_KEY_LEN {
        return Err(RegistryError::DecryptFailed(
            "LSA key blob shorter than expected".to_string(),
        ));
    }
    Ok(Zeroizing::new(
        blob[LSA_KEY_OFFSET..LSA_KEY_OFFSET + LSA_KEY_LEN].to_vec(),
    ))
}

/// Decrypt a single NT6 LSA secret value (e.g. a `CurrVal` default value).
pub(super) fn decrypt_lsa_secret(
    lsa_key: &[u8],
    value: &[u8],
) -> Result<Zeroizing<Vec<u8>>, RegistryError> {
    let data = lsa_secret_data(value)?;
    let round_key = sha256_expand(lsa_key, &data[..KEY_MATERIAL_LEN]);
    let decrypted = aes256_ecb_decrypt(&round_key, &data[KEY_MATERIAL_LEN..]);
    Ok(Zeroizing::new(parse_secret_blob(&decrypted)?.to_vec()))
}

/// Return the `data` section of an LSA_SECRET record (after the 28-byte header).
fn lsa_secret_data(value: &[u8]) -> Result<&[u8], RegistryError> {
    if value.len() < LSA_SECRET_HEADER_LEN + KEY_MATERIAL_LEN {
        return Err(RegistryError::truncated(
            value.len(),
            "LSA secret record shorter than header plus key material",
        ));
    }
    Ok(&value[LSA_SECRET_HEADER_LEN..])
}

/// `SHA-256(key)` expanded by re-feeding the 32-byte material 1000 times.
fn sha256_expand(key: &[u8], material: &[u8]) -> [u8; 32] {
    let mut context = Sha256::new();
    context.update(key);
    for _ in 0..SHA256_EXPANSION_ROUNDS {
        context.update(material);
    }
    context.finalize().into()
}

/// AES-256 ECB decryption with the final short block zero-padded.
fn aes256_ecb_decrypt(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let cipher = aes::Aes256::new_from_slice(key).expect("AES-256 key length is 32 bytes");
    let mut plaintext = Vec::with_capacity(data.len() + 16);
    for chunk in data.chunks(16) {
        let mut block = [0u8; 16];
        block[..chunk.len()].copy_from_slice(chunk);
        let mut block = GenericArray::clone_from_slice(&block);
        cipher.decrypt_block(&mut block);
        plaintext.extend_from_slice(&block);
    }
    plaintext
}

/// Read the `length(4) | reserved(12) | secret[length]` blob.
fn parse_secret_blob(decrypted: &[u8]) -> Result<&[u8], RegistryError> {
    if decrypted.len() < BLOB_HEADER_LEN {
        return Err(RegistryError::truncated(
            decrypted.len(),
            "LSA secret blob header truncated",
        ));
    }
    let length =
        u32::from_le_bytes(decrypted[..4].try_into().expect("length field is 4 bytes")) as usize;
    let end = BLOB_HEADER_LEN.saturating_add(length);
    if decrypted.len() < end {
        return Err(RegistryError::truncated(
            decrypted.len(),
            "LSA secret blob payload truncated",
        ));
    }
    Ok(&decrypted[BLOB_HEADER_LEN..end])
}
