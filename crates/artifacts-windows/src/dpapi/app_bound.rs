//! Chrome App-Bound Encryption offline unwrapping primitives.
//!
//! The Chrome key blob content starts with a flag byte selecting the scheme:
//! - `0x01` (Chrome ~127-132): AES-256-GCM with a build-embedded key.
//! - `0x02` (Chrome 133-136): ChaCha20-Poly1305 with a build-embedded key.
//! - `0x03` (Chrome 137+): CNG `Google Chromekey1` AES key -> AES-256-CBC
//!   (zero IV) -> XOR with a build-embedded constant -> AES-256-GCM.
//!
//! Known keys come from public reverse engineering (runassu's
//! chrome_v20_decryption PoC, Invoke-PowerChrome, independent writeups). The
//! AEAD tag is always the final arbiter: every known candidate is tried and
//! only an authenticated result is accepted. When the matching
//! `elevation_service.exe` bytes are available, a candidate must additionally
//! occur exactly once in the binary to be reported as build-bound.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaChaNonce};
use zeroize::Zeroizing;

use super::algorithms::{decrypt_cbc_no_padding, CipherAlgorithm};
use super::error::DpapiError;

/// Fixed DPAPI entropy for the CNG key file's properties blob.
pub const KSP_PROPERTY_ENTROPY: &[u8] = b"6jnkd5J3ZdQDtrsu\x00";
/// Fixed DPAPI entropy for the CNG key file's private key blob.
pub const KSP_PRIVATE_KEY_ENTROPY: &[u8] = b"xT5rZW5qVVbrvpuA\x00";

const CNG_HEADER_LEN: usize = 44;
const DIRECT_BLOB_LEN: usize = 61;
const FLAG3_LEN: usize = 93;
const APP_BOUND_KEY_LEN: usize = 32;

const FLAG_AES_GCM_DIRECT: u8 = 0x01;
const FLAG_CHACHA20_DIRECT: u8 = 0x02;
const FLAG_CNG_XOR_AES_GCM: u8 = 0x03;

/// App-Bound key protection scheme selected by the content flag byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppBoundScheme {
    AesGcmDirect,
    ChaCha20Direct,
    CngXorAesGcm,
}

impl AppBoundScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AesGcmDirect => "aes-256-gcm-direct",
            Self::ChaCha20Direct => "chacha20-poly1305-direct",
            Self::CngXorAesGcm => "cng-xor-aes-256-gcm",
        }
    }
}

/// A publicly documented App-Bound key constant with its provenance.
pub struct AppBoundKeyCandidate {
    pub scheme: AppBoundScheme,
    pub chrome_versions: &'static str,
    pub source: &'static str,
    pub key: [u8; 32],
}

/// XOR constant documented for Chrome 137+ builds (including 147.0.7727.102).
pub const CHROME_147_XOR_CONSTANT: [u8; 32] = [
    0xcc, 0xf8, 0xa1, 0xce, 0xc5, 0x66, 0x05, 0xb8, 0x51, 0x75, 0x52, 0xba, 0x1a, 0x2d, 0x06, 0x1c,
    0x03, 0xa2, 0x9e, 0x90, 0x27, 0x4f, 0xb2, 0xfc, 0xf5, 0x9b, 0xa4, 0xb7, 0x5c, 0x39, 0x23, 0x90,
];

/// Known App-Bound key constants from public reverse engineering.
pub const KNOWN_APP_BOUND_KEYS: &[AppBoundKeyCandidate] = &[
    AppBoundKeyCandidate {
        scheme: AppBoundScheme::AesGcmDirect,
        chrome_versions: "127-132",
        source: "runassu/chrome_v20_decryption (elevation_service.exe)",
        key: [
            0xb3, 0x1c, 0x6e, 0x24, 0x1a, 0xc8, 0x46, 0x72, 0x8d, 0xa9, 0xc1, 0xfa, 0xc4, 0x93,
            0x66, 0x51, 0xcf, 0xfb, 0x94, 0x4d, 0x14, 0x3a, 0xb8, 0x16, 0x27, 0x6b, 0xcc, 0x6d,
            0xa0, 0x28, 0x47, 0x87,
        ],
    },
    AppBoundKeyCandidate {
        scheme: AppBoundScheme::ChaCha20Direct,
        chrome_versions: "133-136",
        source: "runassu/chrome_v20_decryption (elevation_service.exe)",
        key: [
            0xe9, 0x8f, 0x37, 0xd7, 0xf4, 0xe1, 0xfa, 0x43, 0x3d, 0x19, 0x30, 0x4d, 0xc2, 0x25,
            0x80, 0x42, 0x09, 0x0e, 0x2d, 0x1d, 0x7e, 0xea, 0x76, 0x70, 0xd4, 0x1f, 0x73, 0x8d,
            0x08, 0x72, 0x96, 0x60,
        ],
    },
    AppBoundKeyCandidate {
        scheme: AppBoundScheme::CngXorAesGcm,
        chrome_versions: "137+",
        source: "runassu/chrome_v20_decryption, Invoke-PowerChrome, somesota.blog",
        key: CHROME_147_XOR_CONSTANT,
    },
];

/// Result of a successful App-Bound unwrap with provenance.
pub struct UnwrappedAppBoundKey {
    pub key: Zeroizing<[u8; 32]>,
    pub scheme: AppBoundScheme,
    pub bound_to_elevation: bool,
}

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

/// Parse the CNG system key container: seven little-endian u32 fields in a
/// 44-byte header followed by description (UTF-16LE), public key, properties
/// blob, and private blob. The field lengths must consume the file exactly.
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

/// Whether the Chrome key blob content requires the CNG `Google Chromekey1`
/// file (only the flag-3 scheme does).
pub fn content_requires_cng(content: &[u8]) -> bool {
    content.first().copied() == Some(FLAG_CNG_XOR_AES_GCM)
}

/// Unwrap the 32-byte App-Bound key from Chrome key blob content. Every known
/// candidate key for the scheme is tried; only an AEAD-authenticated result
/// is accepted. With `elevation_exe` present, the winning candidate must occur
/// exactly once in the binary.
pub fn unwrap_app_bound_key(
    content: &[u8],
    cng_aes_key: Option<&[u8; 32]>,
    elevation_exe: Option<&[u8]>,
) -> Result<UnwrappedAppBoundKey, DpapiError> {
    match content.first().copied() {
        Some(FLAG_AES_GCM_DIRECT) => {
            unwrap_direct(content, AppBoundScheme::AesGcmDirect, elevation_exe)
        }
        Some(FLAG_CHACHA20_DIRECT) => {
            unwrap_direct(content, AppBoundScheme::ChaCha20Direct, elevation_exe)
        }
        Some(FLAG_CNG_XOR_AES_GCM) => unwrap_flag3(content, cng_aes_key, elevation_exe),
        Some(flag) => Err(DpapiError::UnsupportedVersion(u32::from(flag))),
        None => Err(DpapiError::TooShort {
            needed: 1,
            actual: 0,
        }),
    }
}

fn unwrap_direct(
    content: &[u8],
    scheme: AppBoundScheme,
    elevation_exe: Option<&[u8]>,
) -> Result<UnwrappedAppBoundKey, DpapiError> {
    if content.len() != DIRECT_BLOB_LEN {
        return Err(DpapiError::InvalidFormat(
            "unexpected Chrome app-bound direct blob length",
        ));
    }
    let (candidates, bound) = candidates_for(scheme, elevation_exe)?;
    let nonce = &content[1..13];
    let ciphertext = &content[13..DIRECT_BLOB_LEN];
    for candidate in candidates {
        let plaintext = match scheme {
            AppBoundScheme::AesGcmDirect => {
                aes_gcm_decrypt_slice(&candidate.key, nonce, ciphertext)
            }
            AppBoundScheme::ChaCha20Direct => chacha20_decrypt(&candidate.key, nonce, ciphertext),
            AppBoundScheme::CngXorAesGcm => continue,
        };
        if let Some(key) = authenticated_key(plaintext) {
            return Ok(UnwrappedAppBoundKey {
                key,
                scheme,
                bound_to_elevation: bound,
            });
        }
    }
    Err(DpapiError::IntegrityMismatch)
}

fn unwrap_flag3(
    content: &[u8],
    cng_aes_key: Option<&[u8; 32]>,
    elevation_exe: Option<&[u8]>,
) -> Result<UnwrappedAppBoundKey, DpapiError> {
    let cng_aes_key = cng_aes_key.ok_or(DpapiError::InvalidFormat(
        "CNG system key is required for the Chrome flag-3 scheme",
    ))?;
    if content.len() != FLAG3_LEN {
        return Err(DpapiError::InvalidFormat("expected Chrome flag-3 key blob"));
    }
    let (candidates, bound) = candidates_for(AppBoundScheme::CngXorAesGcm, elevation_exe)?;
    let ncrypt_plaintext = decrypt_cbc_no_padding(
        CipherAlgorithm::Aes256,
        cng_aes_key,
        &[0u8; 16],
        &content[1..33],
    )?;
    let nonce = &content[33..45];
    let ciphertext = &content[45..FLAG3_LEN];
    for candidate in candidates {
        let wrapping_key = ncrypt_plaintext
            .iter()
            .zip(candidate.key.iter())
            .map(|(left, right)| left ^ right)
            .collect::<Vec<u8>>();
        let plaintext = aes_gcm_decrypt_slice(&wrapping_key, nonce, ciphertext);
        if let Some(key) = authenticated_key(plaintext) {
            return Ok(UnwrappedAppBoundKey {
                key,
                scheme: AppBoundScheme::CngXorAesGcm,
                bound_to_elevation: bound,
            });
        }
    }
    Err(DpapiError::IntegrityMismatch)
}

/// Known candidates for a scheme; with the elevation binary present, keep only
/// those occurring exactly once and report the binding.
fn candidates_for(
    scheme: AppBoundScheme,
    elevation_exe: Option<&[u8]>,
) -> Result<(Vec<&'static AppBoundKeyCandidate>, bool), DpapiError> {
    let all = KNOWN_APP_BOUND_KEYS
        .iter()
        .filter(|candidate| candidate.scheme == scheme)
        .collect::<Vec<_>>();
    let Some(exe) = elevation_exe else {
        return Ok((all, false));
    };
    let matched = all
        .into_iter()
        .filter(|candidate| {
            exe.windows(APP_BOUND_KEY_LEN)
                .filter(|window| *window == candidate.key)
                .count()
                == 1
        })
        .collect::<Vec<_>>();
    if matched.is_empty() {
        return Err(DpapiError::InvalidFormat(
            "no known App-Bound key constant found exactly once in elevation service",
        ));
    }
    Ok((matched, true))
}

fn authenticated_key(plaintext: Option<Vec<u8>>) -> Option<Zeroizing<[u8; 32]>> {
    let plaintext = plaintext?;
    if plaintext.len() != APP_BOUND_KEY_LEN {
        return None;
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&plaintext);
    Some(key)
}

fn aes_gcm_decrypt_slice(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher.decrypt(Nonce::from_slice(nonce), ciphertext).ok()
}

fn chacha20_decrypt(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).ok()?;
    cipher
        .decrypt(ChaChaNonce::from_slice(nonce), ciphertext)
        .ok()
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
