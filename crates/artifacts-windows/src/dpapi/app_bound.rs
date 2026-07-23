//! Chrome App-Bound Encryption (flag-3) offline unwrapping primitives.
//!
//! Chain: Chrome key blob (header/content) -> flag-3 layout -> CNG
//! `Google Chromekey1` AES key -> AES-256-CBC (zero IV) -> version-bound XOR
//! constant -> AES-256-GCM -> 32-byte App-Bound key. Every step is validated
//! structurally; the GCM tag is the final authentication point.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use zeroize::Zeroizing;

use super::algorithms::{decrypt_cbc_no_padding, CipherAlgorithm};
use super::error::DpapiError;

/// Fixed DPAPI entropy for the CNG key file's properties blob.
pub const KSP_PROPERTY_ENTROPY: &[u8] = b"6jnkd5J3ZdQDtrsu\x00";
/// Fixed DPAPI entropy for the CNG key file's private key blob.
pub const KSP_PRIVATE_KEY_ENTROPY: &[u8] = b"xT5rZW5qVVbrvpuA\x00";

const CNG_HEADER_LEN: usize = 44;
const FLAG3_LEN: usize = 93;
const APP_BOUND_KEY_LEN: usize = 32;

/// XOR constant bound to Chrome `147.0.7727.102` (`elevation_service.exe`).
pub const CHROME_147_XOR_CONSTANT: [u8; 32] = [
    0xcc, 0xf8, 0xa1, 0xce, 0xc5, 0x66, 0x05, 0xb8, 0x51, 0x75, 0x52, 0xba, 0x1a, 0x2d, 0x06, 0x1c,
    0x03, 0xa2, 0x9e, 0x90, 0x27, 0x4f, 0xb2, 0xfc, 0xf5, 0x9b, 0xa4, 0xb7, 0x5c, 0x39, 0x23, 0x90,
];

/// Parsed Chrome custom key blob: validation path plus the wrapped content.
#[derive(Debug, Clone)]
pub struct ChromeKeyBlob {
    pub validation_path: String,
    pub content: Vec<u8>,
}

/// Parse the `uint32 header_len | header | uint32 content_len | content`
/// structure produced by the inner App-Bound DPAPI layer.
pub fn parse_chrome_key_blob(bytes: &[u8]) -> Result<ChromeKeyBlob, DpapiError> {
    if bytes.len() < 8 {
        return Err(DpapiError::TooShort {
            needed: 8,
            actual: bytes.len(),
        });
    }
    let header_len = read_u32(bytes, 0)? as usize;
    let header_end = 4usize
        .checked_add(header_len)
        .ok_or(DpapiError::InvalidFormat("Chrome key blob length overflow"))?;
    if bytes.len() < header_end + 4 {
        return Err(DpapiError::TooShort {
            needed: header_end + 4,
            actual: bytes.len(),
        });
    }
    let header = &bytes[4..header_end];
    if header.first().copied() != Some(0x02) {
        return Err(DpapiError::InvalidFormat(
            "unexpected Chrome validation header",
        ));
    }
    let validation_path = String::from_utf8(header[1..].to_vec())
        .map_err(|_| DpapiError::InvalidFormat("Chrome validation path is not UTF-8"))?;
    let content_len = read_u32(bytes, header_end)? as usize;
    let content = &bytes[header_end + 4..];
    if content.len() != content_len {
        return Err(DpapiError::InvalidFormat("Chrome key blob length mismatch"));
    }
    Ok(ChromeKeyBlob {
        validation_path,
        content: content.to_vec(),
    })
}

/// Parsed CNG system key file (`Google Chromekey1`) regions.
#[derive(Debug, Clone)]
pub struct CngSystemKeyFile {
    pub description: String,
    pub properties_blob: Vec<u8>,
    pub private_blob: Vec<u8>,
}

/// Parse the CNG system key container: seven little-endian u32 fields followed
/// by description (UTF-16LE), public key, properties blob, and private blob.
/// The field lengths must consume the file exactly.
pub fn parse_cng_system_key_file(bytes: &[u8]) -> Result<CngSystemKeyFile, DpapiError> {
    if bytes.len() < CNG_HEADER_LEN {
        return Err(DpapiError::TooShort {
            needed: CNG_HEADER_LEN,
            actual: bytes.len(),
        });
    }
    let version = read_u32(bytes, 0)?;
    if version != 1 {
        return Err(DpapiError::InvalidFormat("unsupported CNG key version"));
    }
    let description_len = read_u32(bytes, 8)? as usize;
    let public_len = read_u32(bytes, 16)? as usize;
    let properties_len = read_u32(bytes, 20)? as usize;
    let private_len = read_u32(bytes, 24)? as usize;

    let mut cursor = CNG_HEADER_LEN;
    let description_raw = take(bytes, &mut cursor, description_len)?;
    cursor = cursor
        .checked_add(public_len)
        .ok_or(DpapiError::InvalidFormat("CNG key file length overflow"))?;
    let properties_blob = take(bytes, &mut cursor, properties_len)?.to_vec();
    let private_blob = take(bytes, &mut cursor, private_len)?.to_vec();
    if cursor != bytes.len() {
        return Err(DpapiError::InvalidFormat(
            "CNG key file field lengths do not match file size",
        ));
    }
    let units = description_raw
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    let description = String::from_utf16(&units)
        .map_err(|_| DpapiError::InvalidFormat("CNG description is not UTF-16LE"))?;
    Ok(CngSystemKeyFile {
        description,
        properties_blob,
        private_blob,
    })
}

/// Validate the decrypted CNG private key plaintext and extract the 32-byte
/// AES key. Layout: `"KDBM" | version(4)==1 | key_len(4)==32 | key[32]`.
pub fn parse_cng_private_key(plaintext: &[u8]) -> Result<Zeroizing<[u8; 32]>, DpapiError> {
    if plaintext.len() != 12 + APP_BOUND_KEY_LEN || &plaintext[..4] != b"KDBM" {
        return Err(DpapiError::InvalidFormat(
            "unexpected CNG private key format",
        ));
    }
    let version = read_u32(plaintext, 4)?;
    let key_len = read_u32(plaintext, 8)? as usize;
    if version != 1 || key_len != APP_BOUND_KEY_LEN {
        return Err(DpapiError::InvalidFormat("unexpected CNG AES key format"));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&plaintext[12..]);
    Ok(key)
}

/// Select the version-bound XOR constant. When the matching
/// `elevation_service.exe` bytes are available, the constant must occur exactly
/// once in the binary; without them the known constant is used but reported as
/// unbound so callers can mark the provenance.
pub fn select_xor_constant(elevation_exe: Option<&[u8]>) -> Result<([u8; 32], bool), DpapiError> {
    let Some(exe) = elevation_exe else {
        return Ok((CHROME_147_XOR_CONSTANT, false));
    };
    let occurrences = exe
        .windows(CHROME_147_XOR_CONSTANT.len())
        .filter(|window| *window == CHROME_147_XOR_CONSTANT)
        .count();
    if occurrences != 1 {
        return Err(DpapiError::InvalidFormat(
            "Chrome XOR constant not found exactly once in elevation service",
        ));
    }
    Ok((CHROME_147_XOR_CONSTANT, true))
}

/// Unwrap the 32-byte App-Bound key from a flag-3 content blob:
/// `0x03 | encrypted_aes_key(32) | nonce(12) | ciphertext(32) | tag(16)`.
pub fn unwrap_app_bound_key(
    cng_aes_key: &[u8; 32],
    flag3_content: &[u8],
    xor_constant: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, DpapiError> {
    if flag3_content.len() != FLAG3_LEN || flag3_content.first().copied() != Some(0x03) {
        return Err(DpapiError::InvalidFormat("expected Chrome flag-3 key blob"));
    }
    let ncrypt_plaintext = decrypt_cbc_no_padding(
        CipherAlgorithm::Aes256,
        cng_aes_key,
        &[0u8; 16],
        &flag3_content[1..33],
    )?;
    let wrapping_key = ncrypt_plaintext
        .iter()
        .zip(xor_constant.iter())
        .map(|(left, right)| left ^ right)
        .collect::<Vec<u8>>();
    let cipher =
        Aes256Gcm::new_from_slice(&wrapping_key).map_err(|_| DpapiError::InvalidKeyLength)?;
    let nonce = Nonce::from_slice(&flag3_content[33..45]);
    let plaintext = cipher
        .decrypt(nonce, &flag3_content[45..FLAG3_LEN])
        .map_err(|_| DpapiError::IntegrityMismatch)?;
    if plaintext.len() != APP_BOUND_KEY_LEN {
        return Err(DpapiError::InvalidFormat(
            "unexpected Chrome app-bound key length",
        ));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&plaintext);
    Ok(key)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, DpapiError> {
    let slice = bytes.get(offset..offset + 4).ok_or(DpapiError::TooShort {
        needed: offset + 4,
        actual: bytes.len(),
    })?;
    Ok(u32::from_le_bytes(slice.try_into().map_err(|_| {
        DpapiError::InvalidFormat("invalid 32-bit field")
    })?))
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], DpapiError> {
    let end = cursor
        .checked_add(len)
        .ok_or(DpapiError::InvalidFormat("length overflow"))?;
    let slice = bytes.get(*cursor..end).ok_or(DpapiError::TooShort {
        needed: end,
        actual: bytes.len(),
    })?;
    *cursor = end;
    Ok(slice)
}
