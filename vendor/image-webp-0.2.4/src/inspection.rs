//! Native container and image-header inspection without raster allocation.

pub use crate::decoder::{read_chunk_header, WebPRiffChunk};
use crate::decoder::{read_fourcc, DecodingError};
use byteorder_lite::{LittleEndian, ReadBytesExt};
use std::io::BufRead;

/// Reads the native RIFF/WEBP header and returns the declared container end.
pub fn read_container_header<R: BufRead>(mut r: R) -> Result<u64, DecodingError> {
    let (WebPRiffChunk::RIFF, size, _) = read_chunk_header(&mut r)? else {
        return Err(DecodingError::ChunkHeaderInvalid(*b"RIFF"));
    };
    match read_fourcc(&mut r)? {
        WebPRiffChunk::WEBP => {}
        fourcc => return Err(DecodingError::WebpSignatureInvalid(fourcc.to_fourcc())),
    }
    if size < 4 {
        return Err(DecodingError::InvalidChunkSize);
    }
    Ok(8 + size)
}

/// Reads dimensions from a VP8, VP8L or VP8X payload using native header readers.
pub fn read_dimensions<R: BufRead>(
    mut r: R,
    kind: WebPRiffChunk,
) -> Result<(u32, u32), DecodingError> {
    match kind {
        WebPRiffChunk::VP8 => read_lossy_header(&mut r),
        WebPRiffChunk::VP8L => {
            let (width, height, _) = read_lossless_header(&mut r)?;
            Ok((width, height))
        }
        WebPRiffChunk::VP8X => {
            let info = crate::extended::read_extended_header(&mut r)?;
            Ok((info.canvas_width, info.canvas_height))
        }
        _ => Err(DecodingError::ChunkHeaderInvalid(kind.to_fourcc())),
    }
}

pub(crate) fn read_lossy_header<R: BufRead>(mut r: R) -> Result<(u32, u32), DecodingError> {
    let tag = r.read_u24::<LittleEndian>()?;
    if tag & 1 != 0 {
        return Err(DecodingError::UnsupportedFeature(
            "Non-keyframe frames".to_owned(),
        ));
    }
    let mut tag = [0u8; 3];
    r.read_exact(&mut tag)?;
    if tag != [0x9d, 0x01, 0x2a] {
        return Err(DecodingError::Vp8MagicInvalid(tag));
    }
    let width = u32::from(r.read_u16::<LittleEndian>()? & 0x3fff);
    let height = u32::from(r.read_u16::<LittleEndian>()? & 0x3fff);
    if width == 0 || height == 0 {
        return Err(DecodingError::InconsistentImageSizes);
    }
    Ok((width, height))
}

pub(crate) fn read_lossless_header<R: BufRead>(
    mut r: R,
) -> Result<(u32, u32, bool), DecodingError> {
    let signature = r.read_u8()?;
    if signature != 0x2f {
        return Err(DecodingError::LosslessSignatureInvalid(signature));
    }
    let header = r.read_u32::<LittleEndian>()?;
    let version = header >> 29;
    if version != 0 {
        return Err(DecodingError::VersionNumberInvalid(version as u8));
    }
    Ok((
        (header & 0x3fff) + 1,
        ((header >> 14) & 0x3fff) + 1,
        (header >> 28) & 1 != 0,
    ))
}
