use crate::{Result, RocksDbWireError};

use super::model::{BlockCompression, ChecksumType, BLOCK_TRAILER_LENGTH};
use super::{BlockHandle, RangeReader, SstReadOptions};

const XXH3_LAST_BYTE_PRIME: u32 = 0x6b90_83d9;

pub(crate) struct DecodedBlock {
    pub data: Vec<u8>,
    pub compression: BlockCompression,
}

struct VerifiedStoredBlock {
    serialized: Vec<u8>,
    stored_len: usize,
    compression_byte: u8,
}

pub(crate) fn read_exact_range<R: RangeReader>(
    reader: &mut R,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>> {
    if reader.is_cancelled() {
        return Err(RocksDbWireError::SstInspectionCancelled);
    }
    let data = reader
        .read_range(offset, length)
        .map_err(|_error| RocksDbWireError::SstSourceRead { offset, length })?;
    if data.len() != length {
        return Err(RocksDbWireError::SstRangeRead { offset, length });
    }
    Ok(data)
}

pub(crate) fn read_block<R: RangeReader>(
    reader: &mut R,
    handle: BlockHandle,
    checksum_type: ChecksumType,
    options: SstReadOptions,
    compression_dictionary: Option<&[u8]>,
) -> Result<DecodedBlock> {
    let block = read_verified_stored_block(reader, handle, checksum_type, options)?;
    let stored = &block.serialized[..block.stored_len];
    let compression = decode_compression(block.compression_byte, handle.offset)?;
    let data = decompress(
        stored,
        compression,
        handle.offset,
        options.max_decompressed_block_bytes,
        compression_dictionary,
    )?;
    Ok(DecodedBlock { data, compression })
}

pub(crate) fn verify_block_checksum<R: RangeReader>(
    reader: &mut R,
    handle: BlockHandle,
    checksum_type: ChecksumType,
    options: SstReadOptions,
) -> Result<()> {
    read_verified_stored_block(reader, handle, checksum_type, options).map(|_| ())
}

fn read_verified_stored_block<R: RangeReader>(
    reader: &mut R,
    handle: BlockHandle,
    checksum_type: ChecksumType,
    options: SstReadOptions,
) -> Result<VerifiedStoredBlock> {
    if handle.size > options.max_stored_block_bytes as u64 {
        return Err(RocksDbWireError::SstStoredBlockLimit {
            size: handle.size,
            limit: options.max_stored_block_bytes,
        });
    }
    let stored_len =
        usize::try_from(handle.size).map_err(|_| RocksDbWireError::SstStoredBlockLimit {
            size: handle.size,
            limit: options.max_stored_block_bytes,
        })?;
    let serialized_len =
        stored_len
            .checked_add(BLOCK_TRAILER_LENGTH)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "serialized SST block length",
            })?;
    let serialized = read_exact_range(reader, handle.offset, serialized_len)?;
    let compression_byte = serialized[stored_len];
    let expected = u32::from_le_bytes(
        serialized[stored_len + 1..serialized_len]
            .try_into()
            .map_err(|_| RocksDbWireError::InvalidField {
                context: "SST block checksum trailer",
                reason: "fixed32 width",
            })?,
    );
    verify_checksum(
        checksum_type,
        &serialized[..stored_len],
        compression_byte,
        expected,
        handle.offset,
    )?;
    Ok(VerifiedStoredBlock {
        serialized,
        stored_len,
        compression_byte,
    })
}

fn verify_checksum(
    checksum_type: ChecksumType,
    stored: &[u8],
    compression_byte: u8,
    expected: u32,
    offset: u64,
) -> Result<()> {
    let actual = match checksum_type {
        ChecksumType::Xxh3 => {
            let hash = xxhash_rust::xxh3::xxh3_64(stored) as u32;
            hash ^ u32::from(compression_byte).wrapping_mul(XXH3_LAST_BYTE_PRIME)
        }
    };
    if actual != expected {
        return Err(RocksDbWireError::SstChecksumMismatch {
            offset,
            expected,
            actual,
        });
    }
    Ok(())
}

fn decode_compression(value: u8, offset: u64) -> Result<BlockCompression> {
    match value {
        0x00 => Ok(BlockCompression::None),
        0x04 => Ok(BlockCompression::Lz4),
        0x05 => Ok(BlockCompression::Lz4Hc),
        compression_type => Err(RocksDbWireError::UnsupportedSstCompression {
            offset,
            compression_type,
        }),
    }
}

fn decompress(
    stored: &[u8],
    compression: BlockCompression,
    offset: u64,
    limit: usize,
    compression_dictionary: Option<&[u8]>,
) -> Result<Vec<u8>> {
    match compression {
        BlockCompression::None => {
            if stored.len() > limit {
                return Err(RocksDbWireError::SstDecompressedBlockLimit {
                    size: stored.len(),
                    limit,
                });
            }
            Ok(stored.to_vec())
        }
        BlockCompression::Lz4 | BlockCompression::Lz4Hc => {
            decompress_lz4_v2(stored, offset, limit, compression_dictionary)
        }
    }
}

fn decompress_lz4_v2(
    stored: &[u8],
    offset: u64,
    limit: usize,
    compression_dictionary: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let (declared, header_len) = decode_varint32_prefix(stored, offset)?;
    let output_len = declared as usize;
    if output_len > limit {
        return Err(RocksDbWireError::SstDecompressedBlockLimit {
            size: output_len,
            limit,
        });
    }
    let mut output = vec![0; output_len];
    let written = match compression_dictionary {
        Some(dictionary) => lz4_flex::block::decompress_into_with_dict(
            &stored[header_len..],
            &mut output,
            dictionary,
        ),
        None => lz4_flex::block::decompress_into(&stored[header_len..], &mut output),
    }
    .map_err(|_| RocksDbWireError::InvalidSstCompression {
        offset,
        reason: "LZ4 format-v2 payload is invalid",
    })?;
    if written != output_len {
        return Err(RocksDbWireError::InvalidSstCompression {
            offset,
            reason: "LZ4 output length differs from declared length",
        });
    }
    Ok(output)
}

fn decode_varint32_prefix(input: &[u8], offset: u64) -> Result<(u32, usize)> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = *input
            .get(index)
            .ok_or(RocksDbWireError::InvalidSstCompression {
                offset,
                reason: "truncated LZ4 size prefix",
            })?;
        if index == 4 && byte > 0x0f {
            return Err(RocksDbWireError::InvalidSstCompression {
                offset,
                reason: "LZ4 size prefix overflows u32",
            });
        }
        value |= u32::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            if index > 0 && byte == 0 {
                return Err(RocksDbWireError::InvalidSstCompression {
                    offset,
                    reason: "non-canonical LZ4 size prefix",
                });
            }
            return Ok((value, index + 1));
        }
    }
    Err(RocksDbWireError::InvalidSstCompression {
        offset,
        reason: "LZ4 size prefix exceeds five bytes",
    })
}
