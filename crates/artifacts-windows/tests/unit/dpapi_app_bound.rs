use super::chrome::{decrypt_chromium_value, BrowserDecryption};
use super::master_key::DecryptedMasterKey;
use super::{
    parse_chrome_key_blob, parse_cng_private_key, parse_cng_system_key_file, select_xor_constant,
    unwrap_app_bound_key, CHROME_147_XOR_CONSTANT,
};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use cbc::Encryptor;
use cipher::{BlockEncryptMut, KeyIvInit};
use std::collections::HashMap;

fn build_chrome_key_blob(path: &str, content: &[u8]) -> Vec<u8> {
    let mut header = vec![0x02];
    header.extend_from_slice(path.as_bytes());
    let mut blob = (header.len() as u32).to_le_bytes().to_vec();
    blob.extend_from_slice(&header);
    blob.extend_from_slice(&(content.len() as u32).to_le_bytes());
    blob.extend_from_slice(content);
    blob
}

#[test]
fn chrome_key_blob_parses_header_and_content() {
    let blob = build_chrome_key_blob("C:\\Program Files\\Google\\Chrome", &[0x03; 93]);
    let parsed = parse_chrome_key_blob(&blob).expect("parse Chrome key blob");
    assert_eq!(parsed.validation_path, "C:\\Program Files\\Google\\Chrome");
    assert_eq!(parsed.content, vec![0x03; 93]);

    let mut bad_header = blob.clone();
    bad_header[4] = 0x01;
    assert!(parse_chrome_key_blob(&bad_header).is_err());

    let mut bad_length = blob;
    let length = bad_length.len() - 1;
    bad_length.truncate(length);
    assert!(parse_chrome_key_blob(&bad_length).is_err());
    assert!(parse_chrome_key_blob(&[0u8; 4]).is_err());
}

fn build_cng_file(description: &str, properties: &[u8], private: &[u8]) -> Vec<u8> {
    let description_raw = description
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .chain([0, 0])
        .collect::<Vec<_>>();
    let public = vec![0xEE; 8];
    let mut file = Vec::new();
    file.extend_from_slice(&1u32.to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&(description_raw.len() as u32).to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&(public.len() as u32).to_le_bytes());
    file.extend_from_slice(&(properties.len() as u32).to_le_bytes());
    file.extend_from_slice(&(private.len() as u32).to_le_bytes());
    file.extend_from_slice(&[0u8; 16]);
    file.extend_from_slice(&description_raw);
    file.extend_from_slice(&public);
    file.extend_from_slice(properties);
    file.extend_from_slice(private);
    file
}

#[test]
fn cng_system_key_file_requires_exact_layout() {
    let file = build_cng_file("Google Chromekey1", &[0x11; 32], &[0x22; 48]);
    let parsed = parse_cng_system_key_file(&file).expect("parse CNG key file");
    assert_eq!(parsed.description, "Google Chromekey1");
    assert_eq!(parsed.properties_blob, vec![0x11; 32]);
    assert_eq!(parsed.private_blob, vec![0x22; 48]);

    let mut bad_version = file.clone();
    bad_version[..4].copy_from_slice(&2u32.to_le_bytes());
    assert!(parse_cng_system_key_file(&bad_version).is_err());

    let mut trailing = file;
    trailing.push(0);
    assert!(parse_cng_system_key_file(&trailing).is_err());
    assert!(parse_cng_system_key_file(&[0u8; 20]).is_err());
}

#[test]
fn cng_private_key_validates_kdbm_layout() {
    let mut plaintext = b"KDBM".to_vec();
    plaintext.extend_from_slice(&1u32.to_le_bytes());
    plaintext.extend_from_slice(&32u32.to_le_bytes());
    plaintext.extend_from_slice(&[0x77; 32]);
    let key = parse_cng_private_key(&plaintext).expect("parse CNG private key");
    assert_eq!(key.as_slice(), &[0x77; 32]);

    let mut bad_magic = plaintext.clone();
    bad_magic[..4].copy_from_slice(b"XXXX");
    assert!(parse_cng_private_key(&bad_magic).is_err());

    let mut bad_version = plaintext;
    bad_version[4..8].copy_from_slice(&2u32.to_le_bytes());
    assert!(parse_cng_private_key(&bad_version).is_err());
}

#[test]
fn xor_constant_requires_exactly_one_occurrence_when_bound() {
    let (constant, bound) = select_xor_constant(None).expect("constant table fallback");
    assert_eq!(constant, CHROME_147_XOR_CONSTANT);
    assert!(!bound);

    let mut exe = vec![0u8; 4096];
    exe[1024..1056].copy_from_slice(&CHROME_147_XOR_CONSTANT);
    let (_, bound) = select_xor_constant(Some(&exe)).expect("bound constant");
    assert!(bound);

    let without = vec![0u8; 4096];
    assert!(select_xor_constant(Some(&without)).is_err());

    let mut twice = exe;
    twice[2048..2080].copy_from_slice(&CHROME_147_XOR_CONSTANT);
    assert!(select_xor_constant(Some(&twice)).is_err());
}

fn aes256_cbc_encrypt_no_padding(key: &[u8; 32], iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    let mut buffer = plaintext.to_vec();
    let encryptor = Encryptor::<aes::Aes256>::new_from_slices(key, iv).expect("AES-256 CBC");
    let encrypted = encryptor
        .encrypt_padded_mut::<cipher::block_padding::NoPadding>(&mut buffer, plaintext.len())
        .expect("no-padding encryption of full blocks");
    encrypted.to_vec()
}

fn build_flag3(cng_key: &[u8; 32], app_bound_key: &[u8; 32], nonce: &[u8; 12]) -> Vec<u8> {
    let ncrypt_plaintext = CHROME_147_XOR_CONSTANT
        .iter()
        .enumerate()
        .map(|(index, xor)| xor ^ (index as u8).wrapping_add(1))
        .collect::<Vec<u8>>();
    let encrypted_aes_key = aes256_cbc_encrypt_no_padding(cng_key, &[0u8; 16], &ncrypt_plaintext);
    let wrapping_key = ncrypt_plaintext
        .iter()
        .zip(CHROME_147_XOR_CONSTANT.iter())
        .map(|(left, right)| left ^ right)
        .collect::<Vec<u8>>();
    let cipher = Aes256Gcm::new_from_slice(&wrapping_key).expect("wrapping key");
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(nonce), app_bound_key.as_slice())
        .expect("GCM encrypt");
    let mut flag3 = vec![0x03];
    flag3.extend_from_slice(&encrypted_aes_key);
    flag3.extend_from_slice(nonce);
    flag3.extend_from_slice(&ciphertext);
    flag3
}

#[test]
fn flag3_unwrap_recovers_app_bound_key_and_authenticates() {
    let cng_key = [0x42; 32];
    let app_bound_key = [0x07; 32];
    let nonce = [0x01; 12];
    let flag3 = build_flag3(&cng_key, &app_bound_key, &nonce);
    assert_eq!(flag3.len(), 93);

    let key = unwrap_app_bound_key(&cng_key, &flag3, &CHROME_147_XOR_CONSTANT)
        .expect("unwrap app-bound key");
    assert_eq!(key.as_slice(), &app_bound_key);

    let mut tampered = flag3.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(unwrap_app_bound_key(&cng_key, &tampered, &CHROME_147_XOR_CONSTANT).is_err());

    let mut bad_flag = flag3;
    bad_flag[0] = 0x02;
    assert!(unwrap_app_bound_key(&cng_key, &bad_flag, &CHROME_147_XOR_CONSTANT).is_err());
}

#[test]
fn v20_values_decrypt_only_with_app_bound_key() {
    let app_bound_key = [0x07; 32];
    let nonce = [0x09; 12];
    let cipher = Aes256Gcm::new_from_slice(&app_bound_key).expect("app-bound key");
    let mut value = b"v20".to_vec();
    value.extend_from_slice(&nonce);
    value.extend_from_slice(
        &cipher
            .encrypt(Nonce::from_slice(&nonce), b"admin123".as_slice())
            .expect("GCM encrypt"),
    );

    let master_keys = HashMap::new();
    let local_state_key = [0u8; 32];
    let (status, preview, _) = decrypt_chromium_value(
        &value,
        None,
        &local_state_key,
        Some(&app_bound_key),
        &master_keys,
    );
    assert_eq!(status, BrowserDecryption::Decrypted);
    assert_eq!(preview.as_deref(), Some("admin123"));

    let (status, _, _) = decrypt_chromium_value(&value, None, &local_state_key, None, &master_keys);
    assert_eq!(status, BrowserDecryption::Unsupported);

    let mut tampered = value;
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    let (status, _, _) = decrypt_chromium_value(
        &tampered,
        None,
        &local_state_key,
        Some(&app_bound_key),
        &master_keys,
    );
    assert_eq!(status, BrowserDecryption::Failed);
}

#[test]
fn master_key_guid_lookup_is_case_insensitive() {
    let key = DecryptedMasterKey {
        guid: "BE5AEB96-A7E8-4C30-9BF6-3DA141DD6608".to_string(),
        key: [0x5A; 64],
    };
    let decryptor = super::ChromiumDecryptor::from_local_state(
        br#"{"os_crypt":{"encrypted_key":"invalid"}}"#,
        &[key],
    );
    assert!(decryptor.is_err());
}
