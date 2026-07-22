use super::*;
use aes::cipher::BlockEncrypt;

#[test]
fn rc4_crypt_known_vector() {
    let key = [1u8, 2, 3, 4, 5];
    let plaintext = [0u8; 8];
    let ciphertext = rc4_crypt(&key, &plaintext);
    assert_eq!(hex::encode(ciphertext), "b2396305f03dc027");
}

#[test]
fn aes128_cbc_decrypt_known_vector() {
    let key = hex::decode("2b7e151628aed2a6abf7158809cf4f3c").unwrap();
    let iv = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
    let ciphertext = hex::decode("7649abac8119b246cee98e9b12e9197d").unwrap();
    let plaintext =
        aes128_cbc_decrypt(&key, &iv.try_into().unwrap(), &ciphertext).expect("decryption failed");
    assert_eq!(hex::encode(plaintext), "6bc1bee22e409f96e93d7e117393172a");
}

#[test]
fn expand_des_key_known_vector() {
    assert_eq!(
        hex::encode(expand_des_key(&[1, 2, 3, 4, 5, 6, 7])),
        "008080604028180e"
    );
    assert_eq!(hex::encode(expand_des_key(&[0; 7])), "0000000000000000");
}

#[test]
fn rid_to_des_keys_known_vector() {
    let (k1, k2) = rid_to_des_keys(500);
    assert_eq!(hex::encode(k1), "f40040000ea00400");
    assert_eq!(hex::encode(k2), "007a00200006d002");
}

fn aes128_cbc_encrypt(key: &[u8], iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    assert!(key.len() == 16 && plaintext.len().is_multiple_of(16));
    let cipher = aes::Aes128::new_from_slice(key).unwrap();
    let mut prev: [u8; 16] = *iv;
    let mut ciphertext = vec![0u8; plaintext.len()];
    for (i, chunk) in plaintext.chunks_exact(16).enumerate() {
        let mut block = [0u8; 16];
        for j in 0..16 {
            block[j] = chunk[j] ^ prev[j];
        }
        let mut ga = GenericArray::clone_from_slice(&block);
        cipher.encrypt_block(&mut ga);
        ciphertext[i * 16..(i + 1) * 16].copy_from_slice(&ga);
        prev = ga.into();
    }
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

fn des_encrypt_16(key1: &[u8; 8], key2: &[u8; 8], plaintext: &[u8; 16]) -> [u8; 16] {
    let mut ciphertext = [0u8; 16];
    ciphertext[..8].copy_from_slice(&des_encrypt_block(key1, plaintext[..8].try_into().unwrap()));
    ciphertext[8..].copy_from_slice(&des_encrypt_block(key2, plaintext[8..].try_into().unwrap()));
    ciphertext
}

#[test]
fn derive_hashed_boot_key_aes_roundtrip() {
    let boot_key: [u8; 16] = *b"0123456789abcdef";
    let salt: [u8; 16] = *b"abcdefghijklmnop";
    let secret: [u8; 32] = *b"0123456789abcdef0123456789abcdef";
    let encrypted = aes128_cbc_encrypt(&boot_key, &salt, &secret);
    let mut account_f = vec![0u8; DOMAIN_KEY_OFFSET + 32 + encrypted.len()];
    account_f[DOMAIN_KEY_OFFSET] = 2;
    account_f[DOMAIN_KEY_OFFSET + 4..DOMAIN_KEY_OFFSET + 8].copy_from_slice(&[2, 0, 0, 0]);
    account_f[DOMAIN_KEY_OFFSET + 8..DOMAIN_KEY_OFFSET + 12]
        .copy_from_slice(&(encrypted.len() as u32).to_le_bytes());
    account_f[DOMAIN_KEY_OFFSET + 12..DOMAIN_KEY_OFFSET + 16]
        .copy_from_slice(&(encrypted.len() as u32).to_le_bytes());
    account_f[DOMAIN_KEY_OFFSET + 16..DOMAIN_KEY_OFFSET + 32].copy_from_slice(&salt);
    account_f[DOMAIN_KEY_OFFSET + 32..].copy_from_slice(&encrypted);

    let derived = derive_hashed_boot_key(boot_key, &account_f).expect("derive failed");
    assert_eq!(derived, secret);
}

#[test]
fn decrypt_user_hashes_aes_roundtrip() {
    let hashed_boot_key: [u8; 32] = *b"0123456789abcdef0123456789abcdef";
    let rid = 500u32;
    let salt: [u8; 16] = *b"saltsaltsaltsalt";
    let final_nt: [u8; 16] = hex::decode(NT_HASH_EMPTY).unwrap().try_into().unwrap();
    let (k1, k2) = rid_to_des_keys(rid);
    let intermediate = des_encrypt_16(&k1, &k2, &final_nt);
    let encrypted = aes128_cbc_encrypt(&hashed_boot_key[..16], &salt, &intermediate);

    let blob_len = 24 + encrypted.len();
    let mut user_v = vec![0u8; USER_V_HEADER_LEN + blob_len];
    user_v[0xA8..0xAC].copy_from_slice(&0u32.to_le_bytes());
    user_v[0xAC..0xB0].copy_from_slice(&(blob_len as u32).to_le_bytes());
    let blob_offset = USER_V_HEADER_LEN;
    user_v[blob_offset..blob_offset + 2].copy_from_slice(&1u16.to_le_bytes());
    user_v[blob_offset + 2..blob_offset + 4].copy_from_slice(&2u16.to_le_bytes());
    user_v[blob_offset + 4..blob_offset + 8].copy_from_slice(&24u32.to_le_bytes());
    user_v[blob_offset + 8..blob_offset + 24].copy_from_slice(&salt);
    user_v[blob_offset + 24..].copy_from_slice(&encrypted);

    let hashes = decrypt_user_hashes(hashed_boot_key, rid, &user_v).expect("decrypt failed");
    assert_eq!(hashes.nt, NT_HASH_EMPTY);
    assert_eq!(hashes.lm, LM_HASH_EMPTY);
}

#[test]
fn encrypted_hash_format_is_selected_by_revision_not_pek_id() {
    let salt = [0x5au8; 16];
    let encrypted = [0xa5u8; 16];
    let mut blob = vec![0u8; 40];
    blob[..2].copy_from_slice(&1u16.to_le_bytes());
    blob[2..4].copy_from_slice(&2u16.to_le_bytes());
    blob[4..8].copy_from_slice(&24u32.to_le_bytes());
    blob[8..24].copy_from_slice(&salt);
    blob[24..].copy_from_slice(&encrypted);

    match parse_encrypted_hash(&blob, 0, blob.len()).expect("parse SAM_HASH_AES") {
        EncryptedHash::Aes { salt: actual, data } => {
            assert_eq!(actual, salt);
            assert_eq!(data, encrypted);
        }
        EncryptedHash::Rc4(_) => panic!("PekID must not select the hash format"),
    }
}

#[test]
fn encrypted_hash_rejects_unknown_revision() {
    let mut blob = vec![0u8; 40];
    blob[2..4].copy_from_slice(&3u16.to_le_bytes());
    assert!(parse_encrypted_hash(&blob, 0, blob.len()).is_none());
}
