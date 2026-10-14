//! Bounded random-access container traversal through the source-bound preview reader.

use crate::file_service::FileServiceError;
use thiserror::Error;

pub(super) const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_SEGMENTS: usize = 8192;

#[derive(Debug, Error)]
pub(super) enum MetadataInputError {
    #[error("invalid image metadata structure")]
    Corrupt,
    #[error("image metadata exceeds the read or segment limit")]
    Limit,
    #[error(transparent)]
    Read(#[from] FileServiceError),
}

pub(super) struct MetadataInput {
    pub format: &'static str,
    pub dimensions: Option<(u32, u32)>,
    pub exif: Option<Vec<u8>>,
    pub incomplete: bool,
}

pub(super) struct MetadataReader<F> {
    pub size: u64,
    pub read: F,
    remaining: usize,
}

impl<F> MetadataReader<F>
where
    F: FnMut(u64, u32) -> Result<Vec<u8>, FileServiceError>,
{
    pub fn new(size: u64, read: F) -> Self {
        Self {
            size,
            read,
            remaining: MAX_METADATA_BYTES,
        }
    }

    pub fn bytes(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, MetadataInputError> {
        let end = offset
            .checked_add(length as u64)
            .ok_or(MetadataInputError::Corrupt)?;
        if end > self.size {
            return Err(MetadataInputError::Corrupt);
        }
        if length > self.remaining {
            return Err(MetadataInputError::Limit);
        }
        self.remaining -= length;
        let mut bytes = Vec::with_capacity(length);
        while bytes.len() < length {
            let chunk_length = (length - bytes.len()).min(128 * 1024);
            let chunk = (self.read)(offset + bytes.len() as u64, chunk_length as u32)?;
            if chunk.len() != chunk_length {
                return Err(MetadataInputError::Corrupt);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    pub fn container(&mut self) -> Result<Option<MetadataInput>, MetadataInputError> {
        let header = self.bytes(0, self.size.min(16) as usize)?;
        if header.starts_with(b"II*\0") || header.starts_with(b"MM\0*") {
            let length = self.size.min(self.remaining as u64) as usize;
            return Ok(Some(MetadataInput {
                format: "tiff",
                dimensions: None,
                exif: Some(self.bytes(0, length)?),
                incomplete: length as u64 != self.size,
            }));
        }
        if header.starts_with(&[0xff, 0xd8]) {
            return self.jpeg().map(Some);
        }
        if header.starts_with(b"\x89PNG\r\n\x1a\n") {
            return self.png().map(Some);
        }
        if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP") {
            return self.webp(&header).map(Some);
        }
        Ok(None)
    }

    fn jpeg(&mut self) -> Result<MetadataInput, MetadataInputError> {
        let mut output = MetadataInput {
            format: "jpeg",
            dimensions: None,
            exif: None,
            incomplete: false,
        };
        let mut offset = 2u64;
        for _ in 0..MAX_SEGMENTS {
            let mut marker = self.bytes(offset, 2)?;
            if marker[0] != 0xff {
                return Err(MetadataInputError::Corrupt);
            }
            while marker[1] == 0xff {
                offset += 1;
                marker = self.bytes(offset, 2)?;
            }
            offset += 2;
            match marker[1] {
                0xda | 0xd9 => return Ok(output),
                0x01 | 0xd0..=0xd8 => continue,
                0 => return Err(MetadataInputError::Corrupt),
                _ => {}
            }
            let length_bytes = self.bytes(offset, 2)?;
            let length = usize::from(u16::from_be_bytes([length_bytes[0], length_bytes[1]]));
            if length < 2
                || offset
                    .checked_add(length as u64)
                    .is_none_or(|end| end > self.size)
            {
                return Err(MetadataInputError::Corrupt);
            }
            if marker[1] == 0xe1 {
                let payload = self.bytes(offset + 2, length - 2)?;
                if let Some(exif) = payload.strip_prefix(b"Exif\0\0") {
                    if output.exif.is_some() {
                        return Err(MetadataInputError::Corrupt);
                    }
                    output.exif = Some(exif.to_vec());
                }
            } else if matches!(marker[1], 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                if length < 8 {
                    return Err(MetadataInputError::Corrupt);
                }
                let frame = self.bytes(offset + 2, 6)?;
                output.dimensions = Some((
                    u32::from(u16::from_be_bytes([frame[3], frame[4]])),
                    u32::from(u16::from_be_bytes([frame[1], frame[2]])),
                ));
            }
            offset += length as u64;
        }
        Err(MetadataInputError::Limit)
    }

    fn png(&mut self) -> Result<MetadataInput, MetadataInputError> {
        let mut output = MetadataInput {
            format: "png",
            dimensions: None,
            exif: None,
            incomplete: false,
        };
        let mut offset = 8u64;
        for index in 0..MAX_SEGMENTS {
            let header = self.bytes(offset, 8)?;
            let length = u32::from_be_bytes(
                header[..4]
                    .try_into()
                    .map_err(|_| MetadataInputError::Corrupt)?,
            ) as usize;
            let kind = &header[4..];
            let end = offset
                .checked_add(12)
                .and_then(|value| value.checked_add(length as u64))
                .ok_or(MetadataInputError::Corrupt)?;
            if end > self.size || (index == 0 && (kind != b"IHDR" || length != 13)) {
                return Err(MetadataInputError::Corrupt);
            }
            if kind == b"IHDR" || kind == b"eXIf" {
                let payload = self.bytes(offset + 8, length)?;
                let crc = self.bytes(end - 4, 4)?;
                let mut hash = crc32fast::Hasher::new();
                hash.update(kind);
                hash.update(&payload);
                if hash.finalize().to_be_bytes().as_slice() != crc {
                    return Err(MetadataInputError::Corrupt);
                }
                if kind == b"IHDR" {
                    if output.dimensions.is_some() {
                        return Err(MetadataInputError::Corrupt);
                    }
                    output.dimensions = Some((
                        u32::from_be_bytes(
                            payload[..4]
                                .try_into()
                                .map_err(|_| MetadataInputError::Corrupt)?,
                        ),
                        u32::from_be_bytes(
                            payload[4..8]
                                .try_into()
                                .map_err(|_| MetadataInputError::Corrupt)?,
                        ),
                    ));
                } else {
                    if output.exif.is_some() {
                        return Err(MetadataInputError::Corrupt);
                    }
                    output.exif = Some(payload);
                }
            }
            if kind == b"IEND" {
                return if length == 0 {
                    Ok(output)
                } else {
                    Err(MetadataInputError::Corrupt)
                };
            }
            offset = end;
        }
        Err(MetadataInputError::Limit)
    }

    fn webp(&mut self, header: &[u8]) -> Result<MetadataInput, MetadataInputError> {
        let end = u64::from(u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| MetadataInputError::Corrupt)?,
        )) + 8;
        if end > self.size || end < 12 {
            return Err(MetadataInputError::Corrupt);
        }
        let mut output = MetadataInput {
            format: "webp",
            dimensions: None,
            exif: None,
            incomplete: false,
        };
        let mut offset = 12u64;
        for _ in 0..MAX_SEGMENTS {
            if offset == end {
                return Ok(output);
            }
            let chunk = self.bytes(offset, 8)?;
            let length = u32::from_le_bytes(
                chunk[4..8]
                    .try_into()
                    .map_err(|_| MetadataInputError::Corrupt)?,
            ) as usize;
            let next = offset
                .checked_add(8)
                .and_then(|value| value.checked_add(length as u64 + (length % 2) as u64))
                .ok_or(MetadataInputError::Corrupt)?;
            if next > end {
                return Err(MetadataInputError::Corrupt);
            }
            if &chunk[..4] == b"EXIF" {
                if output.exif.is_some() {
                    return Err(MetadataInputError::Corrupt);
                }
                let bytes = self.bytes(offset + 8, length)?;
                output.exif = Some(bytes.strip_prefix(b"Exif\0\0").unwrap_or(&bytes).to_vec());
            } else if &chunk[..4] == b"VP8X" {
                if length != 10 {
                    return Err(MetadataInputError::Corrupt);
                }
                let bytes = self.bytes(offset + 8, 10)?;
                output.dimensions = Some((
                    1 + u32::from_le_bytes([bytes[4], bytes[5], bytes[6], 0]),
                    1 + u32::from_le_bytes([bytes[7], bytes[8], bytes[9], 0]),
                ));
            }
            offset = next;
        }
        Err(MetadataInputError::Limit)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/image_metadata_input.rs"]
mod tests;
