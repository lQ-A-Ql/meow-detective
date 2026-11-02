use crate::errors::{FjallStorageError, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const KEY_FORMAT_VERSION: u8 = 1;
pub const ENVELOPE_FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyPart {
    Bytes(Vec<u8>),
    Text(String),
    U16(u16),
    U32(u32),
    U64(u64),
    InverseU64(u64),
}

pub fn encode_key(family: &str, parts: &[KeyPart]) -> Result<Vec<u8>> {
    let family_bytes = family.as_bytes();
    let family_len =
        u16::try_from(family_bytes.len()).map_err(|_| FjallStorageError::KeyPartTooLarge {
            length: family_bytes.len(),
        })?;
    let mut output = Vec::with_capacity(3 + family_bytes.len() + parts.len() * 5);
    output.push(KEY_FORMAT_VERSION);
    output.extend_from_slice(&family_len.to_be_bytes());
    output.extend_from_slice(family_bytes);
    for part in parts {
        match part {
            KeyPart::Bytes(value) => append_bytes(&mut output, value)?,
            KeyPart::Text(value) => append_bytes(&mut output, value.as_bytes())?,
            KeyPart::U16(value) => {
                output.push(0x11);
                output.extend_from_slice(&value.to_be_bytes());
            }
            KeyPart::U32(value) => {
                output.push(0x12);
                output.extend_from_slice(&value.to_be_bytes());
            }
            KeyPart::U64(value) => {
                output.push(0x13);
                output.extend_from_slice(&value.to_be_bytes());
            }
            KeyPart::InverseU64(value) => {
                output.push(0x14);
                output.extend_from_slice(&(!value).to_be_bytes());
            }
        }
    }
    Ok(output)
}

fn append_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let length = u32::try_from(value.len()).map_err(|_| FjallStorageError::KeyPartTooLarge {
        length: value.len(),
    })?;
    output.push(0x01);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
    Ok(())
}

pub fn decode_key(encoded: &[u8]) -> Result<(String, Vec<KeyPart>)> {
    let mut cursor = Cursor::new(encoded);
    let version = cursor.read_u8()?;
    if version != KEY_FORMAT_VERSION {
        return Err(FjallStorageError::UnsupportedKeyVersion(version));
    }
    let family_length = usize::from(cursor.read_u16()?);
    let family = String::from_utf8(cursor.read_exact(family_length)?.to_vec())
        .map_err(|error| FjallStorageError::InvalidEnvelope(error.to_string()))?;
    let mut parts = Vec::new();
    while cursor.remaining() > 0 {
        let tag = cursor.read_u8()?;
        let part = match tag {
            0x01 => {
                let length = cursor.read_u32()? as usize;
                KeyPart::Bytes(cursor.read_exact(length)?.to_vec())
            }
            0x11 => KeyPart::U16(cursor.read_u16()?),
            0x12 => KeyPart::U32(cursor.read_u32()?),
            0x13 => KeyPart::U64(cursor.read_u64()?),
            0x14 => KeyPart::InverseU64(!cursor.read_u64()?),
            _ => {
                return Err(FjallStorageError::InvalidEnvelope(format!(
                    "unknown key part tag {tag}"
                )))
            }
        };
        parts.push(part);
    }
    Ok((family, parts))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionedEnvelope {
    pub format_version: u16,
    pub record_kind: String,
    pub schema_version: u16,
    pub record_id: String,
    pub source_id: String,
    pub revision: u64,
    pub payload: Vec<u8>,
    pub payload_digest: [u8; 32],
}

impl VersionedEnvelope {
    pub fn new(
        record_kind: impl Into<String>,
        schema_version: u16,
        record_id: impl Into<String>,
        source_id: impl Into<String>,
        revision: u64,
        payload: Vec<u8>,
    ) -> Self {
        let payload_digest = digest(&payload);
        Self {
            format_version: ENVELOPE_FORMAT_VERSION,
            record_kind: record_kind.into(),
            schema_version,
            record_id: record_id.into(),
            source_id: source_id.into(),
            revision,
            payload,
            payload_digest,
        }
    }

    pub fn from_serializable<T: Serialize>(
        record_kind: impl Into<String>,
        schema_version: u16,
        record_id: impl Into<String>,
        source_id: impl Into<String>,
        revision: u64,
        payload: &T,
    ) -> Result<Self> {
        let bytes = postcard::to_allocvec(payload)
            .map_err(|error| FjallStorageError::Serialization(error.to_string()))?;
        Ok(Self::new(
            record_kind,
            schema_version,
            record_id,
            source_id,
            revision,
            bytes,
        ))
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        postcard::to_allocvec(self)
            .map_err(|error| FjallStorageError::Serialization(error.to_string()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let envelope: Self = postcard::from_bytes(bytes)
            .map_err(|error| FjallStorageError::Deserialization(error.to_string()))?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn decode_payload<T: DeserializeOwned>(&self) -> Result<T> {
        postcard::from_bytes(&self.payload)
            .map_err(|error| FjallStorageError::Deserialization(error.to_string()))
    }

    fn validate(&self) -> Result<()> {
        if self.format_version != ENVELOPE_FORMAT_VERSION {
            return Err(FjallStorageError::UnsupportedEnvelopeVersion(
                self.format_version,
            ));
        }
        if self.record_kind.is_empty() || self.record_id.is_empty() || self.source_id.is_empty() {
            return Err(FjallStorageError::InvalidEnvelope(
                "identity fields are empty".to_owned(),
            ));
        }
        if digest(&self.payload) != self.payload_digest {
            return Err(FjallStorageError::EnvelopeDigestMismatch);
        }
        Ok(())
    }
}

pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }
    fn read_exact(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(FjallStorageError::TruncatedKey {
                offset: self.offset,
            })?;
        if end > self.bytes.len() {
            return Err(FjallStorageError::TruncatedKey {
                offset: self.offset,
            });
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }
    fn read_u8(&mut self) -> Result<u8> {
        Ok(*self
            .read_exact(1)?
            .first()
            .ok_or(FjallStorageError::TruncatedKey {
                offset: self.offset,
            })?)
    }
    fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.read_exact(2)?.try_into().map_err(
            |_| FjallStorageError::TruncatedKey {
                offset: self.offset,
            },
        )?))
    }
    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.read_exact(4)?.try_into().map_err(
            |_| FjallStorageError::TruncatedKey {
                offset: self.offset,
            },
        )?))
    }
    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.read_exact(8)?.try_into().map_err(
            |_| FjallStorageError::TruncatedKey {
                offset: self.offset,
            },
        )?))
    }
}
