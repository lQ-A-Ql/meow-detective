use super::algorithms::{
    constant_time_eq, decrypt_cbc_with_padding, derive_expanded_key, CipherAlgorithm, HashAlgorithm,
};
use super::error::DpapiError;
use sha1::{Digest, Sha1};
use uuid::Uuid;

/// Parsed DPAPI blob metadata and encrypted payload.
#[derive(Debug, Clone)]
pub struct DpapiBlob {
    pub master_key_guid: String,
    hash_algorithm_id: u32,
    cipher_algorithm_id: u32,
    salt: Vec<u8>,
    hmac: Vec<u8>,
    ciphertext: Vec<u8>,
    signature: Vec<u8>,
    to_sign: Vec<u8>,
}

/// Parse a Windows DPAPI blob without attempting decryption.
pub fn parse_dpapi_blob(data: &[u8]) -> Result<DpapiBlob, DpapiError> {
    let mut cursor = Cursor::new(data);
    let version = cursor.read_u32()?;
    if version != 1 {
        return Err(DpapiError::UnsupportedVersion(version));
    }
    cursor.skip(16)?;
    let to_sign_start = cursor.position();
    let _mk_version = cursor.read_u32()?;
    let guid = cursor.read_guid()?;
    let _flags = cursor.read_u32()?;
    let description_length = cursor.read_u32()? as usize;
    cursor.skip(description_length)?;
    let cipher_algorithm_id = cursor.read_u32()?;
    let _key_length = cursor.read_u32()?;
    let salt_length = cursor.read_u32()? as usize;
    let salt = cursor.read_vec(salt_length)?;
    let strong_length = cursor.read_u32()? as usize;
    cursor.skip(strong_length)?;
    let hash_algorithm_id = cursor.read_u32()?;
    let _hash_length = cursor.read_u32()?;
    let hmac_length = cursor.read_u32()? as usize;
    let hmac = cursor.read_vec(hmac_length)?;
    let ciphertext_length = cursor.read_u32()? as usize;
    let ciphertext = cursor.read_vec(ciphertext_length)?;
    let signature_length_position = cursor.position();
    let signature_length = cursor.read_u32()? as usize;
    let signature = cursor.read_vec(signature_length)?;
    if cursor.position() != data.len() {
        return Err(DpapiError::InvalidFormat("trailing bytes in DPAPI blob"));
    }
    if hmac.is_empty() || signature.is_empty() || ciphertext.is_empty() {
        return Err(DpapiError::InvalidFormat("empty DPAPI payload field"));
    }
    let _ = HashAlgorithm::from_id(hash_algorithm_id)?;
    let _ = CipherAlgorithm::from_id(cipher_algorithm_id)?;
    Ok(DpapiBlob {
        master_key_guid: guid,
        hash_algorithm_id,
        cipher_algorithm_id,
        salt,
        hmac,
        ciphertext,
        signature,
        to_sign: data[to_sign_start..signature_length_position].to_vec(),
    })
}

impl DpapiBlob {
    /// Decrypt and integrity-check this blob with a recovered 64-byte master key.
    pub fn decrypt(&self, master_key: &[u8]) -> Result<Vec<u8>, DpapiError> {
        self.decrypt_with_entropy(master_key, None)
    }

    /// Decrypt a blob with optional application entropy.
    pub(crate) fn decrypt_with_entropy(
        &self,
        master_key: &[u8],
        entropy: Option<&[u8]>,
    ) -> Result<Vec<u8>, DpapiError> {
        if master_key.is_empty() {
            return Err(DpapiError::InvalidKeyLength);
        }
        let hash_algorithm = HashAlgorithm::from_id(self.hash_algorithm_id)?;
        let cipher_algorithm = CipherAlgorithm::from_id(self.cipher_algorithm_id)?;
        let key_hash = Sha1::digest(master_key).to_vec();
        let iv = vec![0u8; cipher_algorithm.iv_len()];
        for session_key in [
            session_key_type1(hash_algorithm, &key_hash, &self.salt, entropy),
            session_key_type2(hash_algorithm, &key_hash, &self.salt, entropy),
        ] {
            let expanded =
                derive_expanded_key(hash_algorithm, &session_key, cipher_algorithm.key_len());
            let Ok(plaintext) = decrypt_cbc_with_padding(
                cipher_algorithm,
                &expanded[..cipher_algorithm.key_len()],
                &iv,
                &self.ciphertext,
            ) else {
                continue;
            };
            if self.verify_signature(hash_algorithm, &key_hash, entropy)
                || self.verify_signature_type2(hash_algorithm, &key_hash, entropy)
            {
                return Ok(plaintext);
            }
        }
        Err(DpapiError::IntegrityMismatch)
    }

    fn verify_signature(
        &self,
        algorithm: HashAlgorithm,
        key_hash: &[u8],
        entropy: Option<&[u8]>,
    ) -> bool {
        let block_len = algorithm.block_len();
        let mut padded = vec![0u8; block_len];
        let copy_len = key_hash.len().min(block_len);
        padded[..copy_len].copy_from_slice(&key_hash[..copy_len]);
        let ipad: Vec<u8> = padded.iter().map(|byte| byte ^ 0x36).collect();
        let opad: Vec<u8> = padded.iter().map(|byte| byte ^ 0x5c).collect();
        let mut inner_input = ipad;
        inner_input.extend_from_slice(&self.hmac);
        let inner = algorithm.digest(&inner_input);
        let mut outer_input = opad;
        outer_input.extend_from_slice(&inner);
        if let Some(entropy) = entropy {
            outer_input.extend_from_slice(entropy);
        }
        outer_input.extend_from_slice(&self.to_sign);
        constant_time_eq(&algorithm.digest(&outer_input), &self.signature)
    }

    fn verify_signature_type2(
        &self,
        algorithm: HashAlgorithm,
        key_hash: &[u8],
        entropy: Option<&[u8]>,
    ) -> bool {
        let mut input = self.hmac.clone();
        if let Some(entropy) = entropy {
            input.extend_from_slice(entropy);
        }
        input.extend_from_slice(&self.to_sign);
        constant_time_eq(&algorithm.hmac(key_hash, &input), &self.signature)
    }
}

fn session_key_type1(
    algorithm: HashAlgorithm,
    key_hash: &[u8],
    nonce: &[u8],
    entropy: Option<&[u8]>,
) -> Vec<u8> {
    let mut input = nonce.to_vec();
    if let Some(entropy) = entropy {
        input.extend_from_slice(entropy);
    }
    algorithm.hmac(key_hash, &input)
}

fn session_key_type2(
    algorithm: HashAlgorithm,
    key_hash: &[u8],
    nonce: &[u8],
    entropy: Option<&[u8]>,
) -> Vec<u8> {
    let mut input = nonce.to_vec();
    if let Some(entropy) = entropy {
        input.extend_from_slice(entropy);
    }
    algorithm.hmac(key_hash, &input)
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn position(&self) -> usize {
        self.offset
    }

    fn read_u32(&mut self) -> Result<u32, DpapiError> {
        let bytes = self.read_array::<4>()?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_guid(&mut self) -> Result<String, DpapiError> {
        let bytes = self.read_array::<16>()?;
        Ok(Uuid::from_bytes_le(bytes).hyphenated().to_string())
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], DpapiError> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(DpapiError::InvalidFormat("DPAPI offset overflow"))?;
        let slice = self
            .data
            .get(self.offset..end)
            .ok_or(DpapiError::TooShort {
                needed: end,
                actual: self.data.len(),
            })?;
        let mut result = [0u8; N];
        result.copy_from_slice(slice);
        self.offset = end;
        Ok(result)
    }

    fn read_vec(&mut self, length: usize) -> Result<Vec<u8>, DpapiError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(DpapiError::InvalidFormat("DPAPI length overflow"))?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(DpapiError::TooShort {
                needed: end,
                actual: self.data.len(),
            })?;
        self.offset = end;
        Ok(bytes.to_vec())
    }

    fn skip(&mut self, length: usize) -> Result<(), DpapiError> {
        let _ = self.read_vec(length)?;
        Ok(())
    }
}
