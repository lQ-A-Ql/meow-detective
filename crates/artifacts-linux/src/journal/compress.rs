//! Decompression of DATA object payloads.
//!
//! The compression algorithm is recorded in the DATA object's
//! `ObjectHeader.flags` (`OBJECT_COMPRESSED_XZ/LZ4/ZSTD`), not sniffed from
//! magic bytes. Layouts per systemd's `src/basic/compress.c`:
//!
//! - XZ: a complete `.xz` container (LZMA2 filter, `LZMA_CHECK_NONE`) produced
//!   by `lzma_stream_buffer_encode` (`compress_blob_xz`); unlike LZ4 there is
//!   no size prefix. This is the default on systemd v219 (RHEL7/CentOS7).
//! - LZ4: 8-byte little-endian original size, followed by a raw LZ4 block
//!   produced by `LZ4_compress_default` (`compress_blob_lz4`).
//! - Zstd: a plain zstd frame produced by `ZSTD_compress`
//!   (`compress_blob_zstd`).

use std::borrow::Cow;
use std::io::Read;

use super::object::{COMPRESSED_LZ4, COMPRESSED_XZ, COMPRESSED_ZSTD};

/// Upper bound for a single decompressed payload. Legitimate journal fields
/// are far smaller; this only guards against forged size prefixes.
const MAX_DECOMPRESSED: u64 = 64 * 1024 * 1024;

pub(super) enum Payload<'a> {
    Decoded(Cow<'a, [u8]>),
    /// The payload claimed compression but failed to decode.
    Corrupt,
}

pub(super) fn decode(flags: u8, payload: &[u8]) -> Payload<'_> {
    match flags & (COMPRESSED_XZ | COMPRESSED_LZ4 | COMPRESSED_ZSTD) {
        0 => Payload::Decoded(Cow::Borrowed(payload)),
        COMPRESSED_XZ => decode_xz(payload),
        COMPRESSED_LZ4 => decode_lz4(payload),
        COMPRESSED_ZSTD => decode_zstd(payload),
        _ => Payload::Corrupt,
    }
}

fn decode_xz(payload: &[u8]) -> Payload<'static> {
    decode_reader(xz2::read::XzDecoder::new(payload))
}

fn decode_zstd(payload: &[u8]) -> Payload<'static> {
    let decoder = match zstd::stream::read::Decoder::new(payload) {
        Ok(decoder) => decoder,
        Err(_) => return Payload::Corrupt,
    };
    decode_reader(decoder)
}

/// Decompress a stream with a hard output cap: reading past the limit marks
/// the payload corrupt instead of materializing a forged-size bomb.
fn decode_reader<R: Read>(reader: R) -> Payload<'static> {
    let mut buffer = Vec::new();
    if reader
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
