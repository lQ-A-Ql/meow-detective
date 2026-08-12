//! Decompression of DATA object payloads.
//!
//! The compression algorithm is recorded in the DATA object's
//! `ObjectHeader.flags` (`OBJECT_COMPRESSED_XZ/LZ4/ZSTD`), not sniffed from
//! magic bytes. Layouts per systemd's `src/basic/compress.c`:
//!
//! - LZ4: 8-byte little-endian original size, followed by a raw LZ4 block
//!   produced by `LZ4_compress_default` (`compress_blob_lz4`).
//! - Zstd: a plain zstd frame produced by `ZSTD_compress`
//!   (`compress_blob_zstd`).
//! - XZ: an lzma stream; **not supported** — reported so the caller can skip
//!   and count the field instead of mistaking compressed bytes for text.

use std::borrow::Cow;
use std::io::Read;

use super::object::{COMPRESSED_LZ4, COMPRESSED_XZ, COMPRESSED_ZSTD};

/// Upper bound for a single decompressed payload. Legitimate journal fields
/// are far smaller; this only guards against forged size prefixes.
const MAX_DECOMPRESSED: u64 = 64 * 1024 * 1024;

pub(super) enum Payload<'a> {
    Decoded(Cow<'a, [u8]>),
    /// XZ compression was flagged; decompression is not implemented.
    XzUnsupported,
    /// The payload claimed compression but failed to decode.
    Corrupt,
}

pub(super) fn decode(flags: u8, payload: &[u8]) -> Payload<'_> {
    match flags & (COMPRESSED_XZ | COMPRESSED_LZ4 | COMPRESSED_ZSTD) {
        0 => Payload::Decoded(Cow::Borrowed(payload)),
        COMPRESSED_XZ => Payload::XzUnsupported,
        COMPRESSED_LZ4 => decode_lz4(payload),
        COMPRESSED_ZSTD => decode_zstd(payload),
        _ => Payload::Corrupt,
    }
}

fn decode_lz4(payload: &[u8]) -> Payload<'static> {
    if payload.len() < 9 {
        return Payload::Corrupt;
    }
    let original = u64::from_le_bytes(payload[0..8].try_into().unwrap_or([0; 8]));
    if original > MAX_DECOMPRESSED {
        return Payload::Corrupt;
    }
    match lz4_flex::block::decompress(&payload[8..], original as usize) {
        Ok(buffer) if buffer.len() as u64 == original => Payload::Decoded(Cow::Owned(buffer)),
        _ => Payload::Corrupt,
    }
}

fn decode_zstd(payload: &[u8]) -> Payload<'static> {
    let decoder = match zstd::stream::read::Decoder::new(payload) {
        Ok(decoder) => decoder,
        Err(_) => return Payload::Corrupt,
    };
    let mut buffer = Vec::new();
    if decoder
        .take(MAX_DECOMPRESSED + 1)
        .read_to_end(&mut buffer)
        .is_err()
    {
        return Payload::Corrupt;
    }
    if buffer.len() as u64 > MAX_DECOMPRESSED {
        return Payload::Corrupt;
    }
    Payload::Decoded(Cow::Owned(buffer))
}
