use super::algorithms::{decrypt_cbc_no_padding, CipherAlgorithm, HashAlgorithm};
use super::error::DpapiError;
use hmac::{Hmac, Mac};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use uuid::Uuid;

const MASTERKEY_FILE_HEADER_LEN: usize = 128;
const MASTER_KEY_LEN: usize = 64;
const MAX_KDF_ROUNDS: u32 = 1_000_000;
const EMPTY_NT_HASH: [u8; 16] = [
    0x31, 0xd6, 0xcf, 0xe0, 0xd1, 0x6a, 0xe9, 0x31, 0xb7, 0x3c, 0x59, 0xd7, 0xe0, 0xc0, 0x89, 0xc0,
];

/// Parsed DPAPI master-key file sections.
#[derive(Debug, Clone)]
pub struct MasterKeyFile {
    pub version: u32,
    pub guid: String,
    pub master_key: Vec<u8>,
    pub backup_key: Vec<u8>,
    pub credential_history: Vec<u8>,
    pub domain_key: Vec<u8>,
}

/// Recovered master key and the GUID used by DPAPI blobs.
#[derive(Debug, Clone)]
pub struct DecryptedMasterKey {
    pub guid: String,
    pub key: [u8; MASTER_KEY_LEN],
}

#[derive(Debug, Clone)]
struct MasterKeySection {
    _version: u32,
    salt: [u8; 16],
    rounds: u32,
    hash_algorithm_id: u32,
    cipher_algorithm_id: u32,
    encrypted: Vec<u8>,
}

/// Parse the fixed header and length-delimited sections of a Protect file.
pub fn parse_masterkey_file(data: &[u8]) -> Result<MasterKeyFile, DpapiError> {
    if data.len() < MASTERKEY_FILE_HEADER_LEN {
        return Err(DpapiError::TooShort {
            needed: MASTERKEY_FILE_HEADER_LEN,
            actual: data.len(),
        });
    }
    let version = read_u32(data, 0)?;
    let guid = decode_guid_utf16(&data[12..84])?;
    let master_len = read_u64(data, 96)?;
    let backup_len = read_u64(data, 104)?;
    let cred_len = read_u64(data, 112)?;
    let domain_len = read_u64(data, 120)?;
    let mut offset = MASTERKEY_FILE_HEADER_LEN;
    let master_key = take_section(data, &mut offset, master_len)?;
    let backup_key = take_section(data, &mut offset, backup_len)?;
    let credential_history = take_section(data, &mut offset, cred_len)?;
    let domain_key = take_section(data, &mut offset, domain_len)?;
    Ok(MasterKeyFile {
        version,
        guid,
        master_key,
        backup_key,
        credential_history,
        domain_key,
    })
}

/// Recover a user master key by trying the supplied pre-keys against both the
/// primary and backup key sections.
pub fn decrypt_master_key_file(
    data: &[u8],
    prekeys: &[[u8; 20]],
) -> Result<DecryptedMasterKey, DpapiError> {
    let borrowed = prekeys.iter().map(<[u8; 20]>::as_slice).collect::<Vec<_>>();
    decrypt_master_key_file_with_keys(data, &borrowed)
}

/// Like [`decrypt_master_key_file`], but accepts variable-length prekeys such
/// as the 20-byte `DPAPI_SYSTEM` machine/user keys or 16-byte legacy keys.
pub(crate) fn decrypt_master_key_file_with_keys(
    data: &[u8],
    prekeys: &[&[u8]],
) -> Result<DecryptedMasterKey, DpapiError> {
    let file = parse_masterkey_file(data)?;
    let mut first_section_error = None;
    let mut parsed_section = false;

    for section_bytes in [&file.master_key, &file.backup_key] {
        if section_bytes.is_empty() {
            continue;
        }
        let section = match parse_master_key_section(section_bytes) {
            Ok(section) => {
                parsed_section = true;
                section
            }
            Err(error) => {
                first_section_error.get_or_insert(error);
                continue;
            }
        };
        for prekey in prekeys {
            if let Ok(key) = decrypt_master_key_section(&section, prekey) {
                return Ok(DecryptedMasterKey {
                    guid: file.guid.clone(),
                    key,
                });
            }
        }
    }

    if parsed_section {
        Err(DpapiError::NoMatchingMasterKey)
    } else {
        Err(first_section_error.unwrap_or(DpapiError::InvalidFormat(
            "master-key file contains no key section",
        )))
    }
}

/// Derive the offline user pre-keys from a TBAL-recovered
/// `SHA1(UTF-16LE(password))` secret.
///
/// The derived `HMAC-SHA1(password_sha1, UTF-16LE(sid + NUL))` candidate comes
/// first, followed by the raw password-SHA1 itself (matching common tool
/// behavior); only candidates that pass the master-key file's internal HMAC
/// check are accepted downstream.
pub fn derive_user_prekeys_from_password_sha1(
    sid: &str,
    password_sha1: &[u8; 20],
) -> Vec<[u8; 20]> {
    let sid_with_nul = utf16le_with_nul(sid);
    vec![hmac_sha1(password_sha1, &sid_with_nul), *password_sha1]
}

/// Derive the offline user pre-keys available from a SAM NT hash.
///
/// The first two candidates cover the standard and Protected Users paths. If
/// the hash is the canonical empty-password hash, the SHA1(empty-password)
/// candidate is also safe to derive without knowing a plaintext password.
pub fn derive_user_prekeys(sid: &str, nt_hash: &[u8; 16]) -> Vec<[u8; 20]> {
    let sid_with_nul = utf16le_with_nul(sid);
    let sid_utf16 = utf16le(sid);
    let mut keys = Vec::with_capacity(3);
    keys.push(hmac_sha1(nt_hash, &sid_with_nul));

    let mut first = [0u8; 32];
    pbkdf2_hmac_sha256(nt_hash, &sid_utf16, 10_000, &mut first);
    let mut second = [0u8; 32];
    pbkdf2_hmac_sha256(&first, &sid_utf16, 1, &mut second);
    keys.push(hmac_sha1(&second[..16], &sid_with_nul));

    if nt_hash == &EMPTY_NT_HASH {
        let password_sha1 = Sha1::digest(utf16le(""));
        keys.push(hmac_sha1(&password_sha1, &sid_with_nul));
    }
    keys
}

fn parse_master_key_section(data: &[u8]) -> Result<MasterKeySection, DpapiError> {
    if data.len() < 32 {
        return Err(DpapiError::TooShort {
            needed: 32,
            actual: data.len(),
        });
    }
    let version = read_u32(data, 0)?;
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&data[4..20]);
    let rounds = read_u32(data, 20)?;
    if rounds == 0 || rounds > MAX_KDF_ROUNDS {
        return Err(DpapiError::InvalidFormat(
            "invalid DPAPI KDF iteration count",
        ));
    }
    let hash_algorithm_id = read_u32(data, 24)?;
    let cipher_algorithm_id = read_u32(data, 28)?;
    let _ = HashAlgorithm::from_id(hash_algorithm_id)?;
    let _ = CipherAlgorithm::from_id(cipher_algorithm_id)?;
    let encrypted = data[32..].to_vec();
    if encrypted.is_empty() {
        return Err(DpapiError::InvalidFormat("empty DPAPI master-key payload"));
    }
    Ok(MasterKeySection {
        _version: version,
        salt,
        rounds,
        hash_algorithm_id,
        cipher_algorithm_id,
        encrypted,
    })
}

fn decrypt_master_key_section(
    section: &MasterKeySection,
    prekey: &[u8],
) -> Result<[u8; MASTER_KEY_LEN], DpapiError> {
    let hash = HashAlgorithm::from_id(section.hash_algorithm_id)?;
    let cipher = CipherAlgorithm::from_id(section.cipher_algorithm_id)?;
    let derived = derive_dpapi_kdf(
        hash,
        prekey,
        &section.salt,
        cipher.key_len() + cipher.iv_len(),
        section.rounds,
    );
    let plaintext = decrypt_cbc_no_padding(
        cipher,
        &derived[..cipher.key_len()],
        &derived[cipher.key_len()..cipher.key_len() + cipher.iv_len()],
        &section.encrypted,
    )?;
    let digest_len = hash.digest_len();
    if plaintext.len() < 16 + digest_len + MASTER_KEY_LEN {
        return Err(DpapiError::DecryptionFailed);
    }
    let master_offset = plaintext.len() - MASTER_KEY_LEN;
    let hmac_salt = &plaintext[..16];
    let stored = &plaintext[16..16 + digest_len];
    let hmac_key = hash.hmac(prekey, hmac_salt);
    let computed = hash.hmac(&hmac_key, &plaintext[master_offset..]);
    if !super::algorithms::constant_time_eq(stored, &computed[..digest_len]) {
        return Err(DpapiError::IntegrityMismatch);
    }
    let mut result = [0u8; MASTER_KEY_LEN];
    result.copy_from_slice(&plaintext[master_offset..]);
    Ok(result)
}

fn derive_dpapi_kdf(
    hash: HashAlgorithm,
    key: &[u8],
    salt: &[u8],
    output_len: usize,
    rounds: u32,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(output_len + hash.digest_len());
    let mut block = 1u32;
    while output.len() < output_len {
        let mut message = salt.to_vec();
        message.extend_from_slice(&block.to_be_bytes());
        let mut value = hash.hmac(key, &message);
        for _ in 1..rounds {
            let next = hash.hmac(key, &value);
            for (left, right) in value.iter_mut().zip(next) {
                *left ^= right;
            }
        }
        output.extend_from_slice(&value);
        block = block.saturating_add(1);
    }
    output.truncate(output_len);
    output
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], rounds: u32, output: &mut [u8]) {
    let mut block = 1u32;
    let mut written = 0usize;
    while written < output.len() {
        let mut message = salt.to_vec();
        message.extend_from_slice(&block.to_be_bytes());
        let mut mac =
            Hmac::<Sha256>::new_from_slice(password).expect("HMAC accepts arbitrary key lengths");
        mac.update(&message);
        let mut value: [u8; 32] = mac.finalize().into_bytes().into();
        let mut previous = value;
        for _ in 1..rounds {
            let mut mac = Hmac::<Sha256>::new_from_slice(password)
                .expect("HMAC accepts arbitrary key lengths");
            mac.update(&previous);
            let current: [u8; 32] = mac.finalize().into_bytes().into();
            for (left, right) in value.iter_mut().zip(current) {
                *left ^= right;
            }
            previous = current;
        }
        let copy_len = (output.len() - written).min(value.len());
        output[written..written + copy_len].copy_from_slice(&value[..copy_len]);
        written += copy_len;
        block = block.saturating_add(1);
    }
}

fn utf16le(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn utf16le_with_nul(value: &str) -> Vec<u8> {
    let mut result = utf16le(value);
    result.extend_from_slice(&[0, 0]);
    result
}

fn decode_guid_utf16(data: &[u8]) -> Result<String, DpapiError> {
    if data.len() != 72 {
        return Err(DpapiError::InvalidFormat("invalid master-key GUID field"));
    }
    let units = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    let value = String::from_utf16(&units)
        .map_err(|_| DpapiError::InvalidFormat("invalid master-key GUID encoding"))?;
    let guid = Uuid::parse_str(value.trim())
        .map_err(|_| DpapiError::InvalidFormat("invalid master-key GUID"))?;
    Ok(guid.hyphenated().to_string())
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, DpapiError> {
    let bytes = data.get(offset..offset + 4).ok_or(DpapiError::TooShort {
        needed: offset + 4,
        actual: data.len(),
    })?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        DpapiError::InvalidFormat("invalid 32-bit field")
    })?))
}

fn read_u64(data: &[u8], offset: usize) -> Result<usize, DpapiError> {
    let bytes = data.get(offset..offset + 8).ok_or(DpapiError::TooShort {
        needed: offset + 8,
        actual: data.len(),
    })?;
    let value = u64::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| DpapiError::InvalidFormat("invalid 64-bit field"))?,
    );
    usize::try_from(value).map_err(|_| DpapiError::InvalidFormat("section length overflow"))
}

fn take_section(data: &[u8], offset: &mut usize, length: usize) -> Result<Vec<u8>, DpapiError> {
    let end = offset
        .checked_add(length)
        .ok_or(DpapiError::InvalidFormat("section length overflow"))?;
    let section = data.get(*offset..end).ok_or(DpapiError::TooShort {
        needed: end,
        actual: data.len(),
    })?;
    *offset = end;
    Ok(section.to_vec())
}
