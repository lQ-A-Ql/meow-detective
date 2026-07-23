use super::keys::{decrypt_lsa_key, decrypt_lsa_secret};
use super::{DpapiSystemKeys, TbalSecret};
use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use sha2::{Digest, Sha256};

const HEADER_LEN: usize = 28;

fn sha256_expand(key: &[u8], material: &[u8]) -> [u8; 32] {
    let mut context = Sha256::new();
    context.update(key);
    for _ in 0..1000 {
        context.update(material);
    }
    context.finalize().into()
}

fn aes256_ecb_encrypt(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let cipher = aes::Aes256::new_from_slice(key).expect("AES-256 key length is 32 bytes");
    let mut ciphertext = Vec::with_capacity(data.len() + 16);
    for chunk in data.chunks(16) {
        let mut block = [0u8; 16];
        block[..chunk.len()].copy_from_slice(chunk);
        let mut block = GenericArray::clone_from_slice(&block);
        cipher.encrypt_block(&mut block);
        ciphertext.extend_from_slice(&block);
    }
    ciphertext
}

/// Build an NT6 LSA_SECRET record wrapping `secret` with the given raw key.
fn build_record(raw_key: &[u8], material: &[u8; 32], secret: &[u8]) -> Vec<u8> {
    let mut blob = (secret.len() as u32).to_le_bytes().to_vec();
    blob.extend_from_slice(&[0u8; 12]);
    blob.extend_from_slice(secret);
    let round_key = sha256_expand(raw_key, material);
    let mut record = vec![0u8; HEADER_LEN];
    record.extend_from_slice(material);
    record.extend_from_slice(&aes256_ecb_encrypt(&round_key, &blob));
    record
}

#[test]
fn lsa_key_round_trip_recovers_embedded_key() {
    let boot_key = [0x11; 16];
    let material = [0x22; 32];
    let mut secret = vec![0xAB; 52];
    let expected_lsa_key = [0x5Au8; 32];
    secret.extend_from_slice(&expected_lsa_key);
    let record = build_record(&boot_key, &material, &secret);

    let lsa_key = decrypt_lsa_key(&boot_key, &record).expect("decrypt LSA key");
    assert_eq!(lsa_key.as_slice(), &expected_lsa_key);
}

#[test]
fn lsa_secret_round_trip_recovers_payload() {
    let lsa_key = [0x33; 32];
    let material = [0x44; 32];
    let secret: Vec<u8> = (0u8..100).collect();
    let record = build_record(&lsa_key, &material, &secret);

    let decrypted = decrypt_lsa_secret(&lsa_key, &record).expect("decrypt LSA secret");
    assert_eq!(decrypted.as_slice(), secret.as_slice());
}

#[test]
fn lsa_secret_rejects_truncated_record() {
    assert!(decrypt_lsa_key(&[0x11; 16], &[0u8; 10]).is_err());
    assert!(decrypt_lsa_secret(&[0x33; 32], &[0u8; 59]).is_err());
}

#[test]
fn dpapi_system_keys_parse_machine_and_user_keys() {
    let mut raw = vec![0x01, 0x00, 0x00, 0x00];
    raw.extend_from_slice(&[0xAA; 20]);
    raw.extend_from_slice(&[0xBB; 20]);
    let keys = DpapiSystemKeys::from_secret(&raw).expect("parse DPAPI_SYSTEM");
    assert_eq!(keys.machine_key, [0xAA; 20]);
    assert_eq!(keys.user_key, [0xBB; 20]);
    assert_eq!(keys.prekeys().len(), 2);

    assert!(DpapiSystemKeys::from_secret(&raw[..43]).is_err());
    assert!(DpapiSystemKeys::from_secret(&[]).is_err());
}

fn build_tbal_raw(nt_hash: [u8; 16], password_sha1: [u8; 20], duplicate: bool) -> Vec<u8> {
    let mut raw = vec![0u8; 136];
    raw[0x10..0x20].copy_from_slice(&nt_hash);
    raw[0x30..0x44].copy_from_slice(&password_sha1);
    let second = if duplicate { password_sha1 } else { [0xFF; 20] };
    raw[0x44..0x58].copy_from_slice(&second);
    raw
}

#[test]
fn tbal_secret_requires_name_layout_and_matching_copies() {
    let name = "M$_MSV1_0_TBAL_PRIMARY_{22BE8E5B-58B3-4A87-BA71-41B0ECF3A9EA}";
    let nt_hash = [0x87; 16];
    let password_sha1 = [0x06; 20];

    let parsed = TbalSecret::from_secret(name, &build_tbal_raw(nt_hash, password_sha1, true))
        .expect("valid TBAL secret");
    assert_eq!(parsed.nt_hash, nt_hash);
    assert_eq!(parsed.password_sha1, password_sha1);

    assert!(
        TbalSecret::from_secret(name, &build_tbal_raw(nt_hash, password_sha1, false)).is_none()
    );
    assert!(TbalSecret::from_secret(
        "DPAPI_SYSTEM",
        &build_tbal_raw(nt_hash, password_sha1, true)
    )
    .is_none());
    assert!(TbalSecret::from_secret(name, &[0u8; 32]).is_none());
}
