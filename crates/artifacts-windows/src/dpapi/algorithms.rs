use aes::{Aes128, Aes192, Aes256};
use cbc::Decryptor;
use cipher::{
    block_padding::{NoPadding, Pkcs7},
    BlockDecryptMut, KeyIvInit,
};
use des::TdesEde3;
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;
use sha2::Sha512;

use super::error::DpapiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HashAlgorithm {
    Md5,
    Sha1,
    Sha512,
}

impl HashAlgorithm {
    pub(super) fn from_id(id: u32) -> Result<Self, DpapiError> {
        match id {
            0x8003 => Ok(Self::Md5),
            0x8004 | 0x8009 => Ok(Self::Sha1),
            0x800e => Ok(Self::Sha512),
            _ => Err(DpapiError::UnsupportedAlgorithm(id)),
        }
    }

    pub(super) fn digest_len(self) -> usize {
        match self {
            Self::Md5 => 16,
            Self::Sha1 => 20,
            Self::Sha512 => 64,
        }
    }

    pub(super) fn block_len(self) -> usize {
        match self {
            Self::Md5 | Self::Sha1 => 64,
            Self::Sha512 => 128,
        }
    }

    pub(super) fn digest(self, input: &[u8]) -> Vec<u8> {
        match self {
            Self::Md5 => Md5::digest(input).to_vec(),
            Self::Sha1 => Sha1::digest(input).to_vec(),
            Self::Sha512 => Sha512::digest(input).to_vec(),
        }
    }

    pub(super) fn hmac(self, key: &[u8], input: &[u8]) -> Vec<u8> {
        match self {
            Self::Md5 => hmac_md5(key, input),
            Self::Sha1 => hmac_sha1(key, input),
            Self::Sha512 => hmac_sha512(key, input),
        }
    }
}

fn hmac_md5(key: &[u8], input: &[u8]) -> Vec<u8> {
    let Ok(mut mac) = Hmac::<Md5>::new_from_slice(key) else {
        return Vec::new();
    };
    mac.update(input);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha1(key: &[u8], input: &[u8]) -> Vec<u8> {
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(key) else {
        return Vec::new();
    };
    mac.update(input);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha512(key: &[u8], input: &[u8]) -> Vec<u8> {
    let Ok(mut mac) = Hmac::<Sha512>::new_from_slice(key) else {
        return Vec::new();
    };
    mac.update(input);
    mac.finalize().into_bytes().to_vec()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CipherAlgorithm {
    Aes128,
    Aes192,
    Aes256,
    Tdes,
    Rc4,
}

impl CipherAlgorithm {
    pub(super) fn from_id(id: u32) -> Result<Self, DpapiError> {
        match id {
            0x660e => Ok(Self::Aes128),
            0x660f => Ok(Self::Aes192),
            0x6610 => Ok(Self::Aes256),
            0x6603 => Ok(Self::Tdes),
            0x6801 => Ok(Self::Rc4),
            _ => Err(DpapiError::UnsupportedAlgorithm(id)),
        }
    }

    pub(super) fn key_len(self) -> usize {
        match self {
            Self::Aes128 => 16,
            Self::Aes192 => 24,
            Self::Aes256 => 32,
            Self::Tdes => 24,
            Self::Rc4 => 5,
        }
    }

    pub(super) fn iv_len(self) -> usize {
        match self {
            Self::Aes128 | Self::Aes192 | Self::Aes256 => 16,
            Self::Tdes => 8,
            Self::Rc4 => 0,
        }
    }

    pub(super) fn block_len(self) -> usize {
        match self {
            Self::Aes128 | Self::Aes192 | Self::Aes256 => 16,
            Self::Tdes => 8,
            Self::Rc4 => 1,
        }
    }
}

pub(super) fn derive_expanded_key(
    algorithm: HashAlgorithm,
    session_key: &[u8],
    key_len: usize,
) -> Vec<u8> {
    let mut derived = if session_key.len() > algorithm.block_len() {
        algorithm.hmac(session_key, &[])
    } else {
        session_key.to_vec()
    };
    if derived.len() < key_len {
        let mut padded = derived.clone();
        padded.resize(algorithm.block_len(), 0);
        let ipad: Vec<u8> = padded.iter().map(|byte| byte ^ 0x36).collect();
        let opad: Vec<u8> = padded.iter().map(|byte| byte ^ 0x5c).collect();
        derived = algorithm.digest(&ipad);
        derived.extend_from_slice(&algorithm.digest(&opad));
    }
    derived
}

pub(super) fn decrypt_cbc_no_padding(
    algorithm: CipherAlgorithm,
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, DpapiError> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(algorithm.block_len()) {
        return Err(DpapiError::DecryptionFailed);
    }
    match algorithm {
        CipherAlgorithm::Aes128 => decrypt_no_padding::<Aes128>(key, iv, ciphertext),
        CipherAlgorithm::Aes192 => decrypt_no_padding::<Aes192>(key, iv, ciphertext),
        CipherAlgorithm::Aes256 => decrypt_no_padding::<Aes256>(key, iv, ciphertext),
        CipherAlgorithm::Tdes => decrypt_no_padding::<TdesEde3>(key, iv, ciphertext),
        CipherAlgorithm::Rc4 => Ok(rc4_crypt(&key[..key.len().min(5)], ciphertext)),
    }
}

pub(super) fn decrypt_cbc_with_padding(
    algorithm: CipherAlgorithm,
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, DpapiError> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(algorithm.block_len()) {
        return Err(DpapiError::DecryptionFailed);
    }
    match algorithm {
        CipherAlgorithm::Aes128 => decrypt_pkcs7::<Aes128>(key, iv, ciphertext),
        CipherAlgorithm::Aes192 => decrypt_pkcs7::<Aes192>(key, iv, ciphertext),
        CipherAlgorithm::Aes256 => decrypt_pkcs7::<Aes256>(key, iv, ciphertext),
        CipherAlgorithm::Tdes => decrypt_pkcs7::<TdesEde3>(key, iv, ciphertext),
        CipherAlgorithm::Rc4 => Ok(rc4_crypt(&key[..key.len().min(5)], ciphertext)),
    }
}

fn decrypt_no_padding<C>(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, DpapiError>
where
    C: cipher::BlockCipher + cipher::BlockDecryptMut + cipher::KeyInit + cipher::BlockSizeUser,
{
    let mut buffer = ciphertext.to_vec();
    let decryptor =
        Decryptor::<C>::new_from_slices(key, iv).map_err(|_| DpapiError::InvalidKeyLength)?;
    decryptor
        .decrypt_padded_mut::<NoPadding>(&mut buffer)
        .map(|bytes| bytes.to_vec())
        .map_err(|_| DpapiError::DecryptionFailed)
}

fn decrypt_pkcs7<C>(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, DpapiError>
where
    C: cipher::BlockCipher + cipher::BlockDecryptMut + cipher::KeyInit + cipher::BlockSizeUser,
{
    let mut buffer = ciphertext.to_vec();
    let decryptor =
        Decryptor::<C>::new_from_slices(key, iv).map_err(|_| DpapiError::InvalidKeyLength)?;
    decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map(|bytes| bytes.to_vec())
        .map_err(|_| DpapiError::DecryptionFailed)
}

pub(super) fn rc4_crypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return Vec::new();
    }
    let mut state: [u8; 256] = std::array::from_fn(|index| index as u8);
    let mut j = 0u8;
    for index in 0..256 {
        j = j
            .wrapping_add(state[index])
            .wrapping_add(key[index % key.len()]);
        state.swap(index, j as usize);
    }
    let mut i = 0u8;
    let mut j = 0u8;
    data.iter()
        .map(|byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(state[i as usize]);
            state.swap(i as usize, j as usize);
            let index = state[i as usize].wrapping_add(state[j as usize]);
            byte ^ state[index as usize]
        })
        .collect()
}

pub(super) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}
