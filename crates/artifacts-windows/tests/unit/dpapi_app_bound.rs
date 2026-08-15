use super::chrome::{decrypt_chromium_value, BrowserDecryption};
use super::master_key::DecryptedMasterKey;
use super::{
    content_requires_cng, parse_chrome_key_blob, parse_cng_private_key, parse_cng_system_key_file,
    unwrap_app_bound_key, AppBoundScheme, CHROME_147_XOR_CONSTANT, KNOWN_APP_BOUND_KEYS,
};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use cbc::Encryptor;
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaChaNonce};
use cipher::{BlockEncryptMut, KeyIvInit};
use std::collections::HashMap;

const FLAG1_AES_KEY: [u8; 32] = KNOWN_APP_BOUND_KEYS[0].key;
const FLAG2_CHACHA_KEY: [u8; 32] = KNOWN_APP_BOUND_KEYS[1].key;

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
fn candidates_bind_to_elevation_binary_only_when_unique() {
    let mut exe = vec![0u8; 4096];
    exe[1024..1056].copy_from_slice(&CHROME_147_XOR_CONSTANT);
    let flag3 = build_flag3(&[0x42; 32], &[0x07; 32], &[0x01; 12]);

    let unwrapped =
        unwrap_app_bound_key(&flag3, Some(&[0x42; 32]), Some(&exe)).expect("bound flag-3 unwrap");
    assert!(unwrapped.bound_to_elevation);
    assert_eq!(unwrapped._scheme, AppBoundScheme::CngXorAesGcm);

    let without = vec![0u8; 4096];
    assert!(unwrap_app_bound_key(&flag3, Some(&[0x42; 32]), Some(&without)).is_err());

    let mut twice = exe;
    twice[2048..2080].copy_from_slice(&CHROME_147_XOR_CONSTANT);
    assert!(unwrap_app_bound_key(&flag3, Some(&[0x42; 32]), Some(&twice)).is_err());

    let unbound = unwrap_app_bound_key(&flag3, Some(&[0x42; 32]), None).expect("unbound unwrap");
    assert!(!unbound.bound_to_elevation);
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
    assert!(content_requires_cng(&flag3));

    let unwrapped =
        unwrap_app_bound_key(&flag3, Some(&cng_key), None).expect("unwrap app-bound key");
    assert_eq!(unwrapped.key.as_slice(), &app_bound_key);
    assert!(!unwrapped.bound_to_elevation);

    let mut tampered = flag3.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(unwrap_app_bound_key(&tampered, Some(&cng_key), None).is_err());

    let mut bad_flag = flag3;
    bad_flag[0] = 0x02;
    assert!(unwrap_app_bound_key(&bad_flag, Some(&cng_key), None).is_err());

    assert!(
        unwrap_app_bound_key(&build_flag3(&cng_key, &app_bound_key, &nonce), None, None).is_err()
    );
}

fn build_direct_blob(flag: u8, key: &[u8; 32], plaintext: &[u8; 32], nonce: &[u8; 12]) -> Vec<u8> {
    let ciphertext = match flag {
        0x01 => Aes256Gcm::new_from_slice(key)
            .expect("direct key")
            .encrypt(Nonce::from_slice(nonce), plaintext.as_slice())
            .expect("GCM encrypt"),
        0x02 => ChaCha20Poly1305::new_from_slice(key)
            .expect("direct key")
            .encrypt(ChaChaNonce::from_slice(nonce), plaintext.as_slice())
            .expect("ChaCha encrypt"),
        _ => unreachable!("direct blob flags are 0x01/0x02"),
    };
    let mut blob = vec![flag];
    blob.extend_from_slice(nonce);
    blob.extend_from_slice(&ciphertext);
    blob
}

#[test]
fn flag1_and_flag2_direct_blobs_unwrap_with_known_keys() {
    let app_bound_key = [0x07; 32];
    let nonce = [0x02; 12];

    let flag1 = build_direct_blob(0x01, &FLAG1_AES_KEY, &app_bound_key, &nonce);
    assert_eq!(flag1.len(), 61);
    assert!(!content_requires_cng(&flag1));
    let unwrapped = unwrap_app_bound_key(&flag1, None, None).expect("flag-1 unwrap");
    assert_eq!(unwrapped.key.as_slice(), &app_bound_key);
    assert_eq!(unwrapped._scheme, AppBoundScheme::AesGcmDirect);

    let flag2 = build_direct_blob(0x02, &FLAG2_CHACHA_KEY, &app_bound_key, &nonce);
    let unwrapped = unwrap_app_bound_key(&flag2, None, None).expect("flag-2 unwrap");
    assert_eq!(unwrapped.key.as_slice(), &app_bound_key);
    assert_eq!(unwrapped._scheme, AppBoundScheme::ChaCha20Direct);

    let mut tampered = flag1;
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(unwrap_app_bound_key(&tampered, None, None).is_err());
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

#[test]
fn non_chrome_direct_key_blob_requires_exact_key_length() {
    let content = [0x6A; 32];
    let key = super::unwrap_direct_key_blob(&content).expect("raw Edge-style key");
    assert_eq!(key.as_slice(), &content);
    assert!(super::unwrap_direct_key_blob(&content[..31]).is_err());
    assert!(super::unwrap_direct_key_blob(&[]).is_err());
}
