use crate::{
    bluestore_semantic::{
        budget::SemanticBudget,
        denc::read_varint_u64,
        types::{BlueStoreChecksum, BlueStoreChecksumType},
    },
    codec::CephDecode,
    crc32c::ceph_crc32c,
    cursor::CephCursor,
    error::{CephWireError, Result},
};
use sha2::{Digest, Sha256};

pub(crate) fn decode_checksum(
    cursor: &mut CephCursor<'_>,
    checksum_coverage_length: u32,
    budget: &mut SemanticBudget,
) -> Result<(BlueStoreChecksum, Vec<u64>)> {
    let raw_type = u8::decode(cursor)?;
    let checksum_type = decode_checksum_type(raw_type)?;
    let chunk_order = u8::decode(cursor)?;
    if chunk_order >= u32::BITS as u8 {
        return Err(invalid_checksum(raw_type, "chunk order must be below 32"));
    }
    let encoded_length =
        usize::try_from(read_varint_u64(cursor, "BlueStore checksum data length")?).map_err(
            |_| CephWireError::IntegerOverflow {
                context: "BlueStore checksum data length",
            },
        )?;
    budget.claim_checksum_bytes(encoded_length)?;
    let word_size = checksum_word_size(checksum_type);
    validate_checksum_length(
        raw_type,
        word_size,
        chunk_order,
        encoded_length,
        checksum_coverage_length,
    )?;
    let word_count = encoded_length / word_size;
    budget.claim_checksum_words(word_count)?;
    let data = cursor.read_exact(encoded_length)?;
    let checksum = BlueStoreChecksum {
        checksum_type,
        chunk_order,
        encoded_length,
        data_ceph_crc32c: ceph_crc32c(data),
        data_sha256: Sha256::digest(data).into(),
    };
    Ok((checksum, decode_words(data, word_size)))
}

fn decode_checksum_type(raw: u8) -> Result<BlueStoreChecksumType> {
    match raw {
        2 => Ok(BlueStoreChecksumType::XxHash32),
        3 => Ok(BlueStoreChecksumType::XxHash64),
        4 => Ok(BlueStoreChecksumType::Crc32c),
        5 => Ok(BlueStoreChecksumType::Crc32c16),
        6 => Ok(BlueStoreChecksumType::Crc32c8),
        checksum_type => Err(CephWireError::UnknownBlueStoreChecksumType { checksum_type }),
    }
}

fn checksum_word_size(checksum_type: BlueStoreChecksumType) -> usize {
    match checksum_type {
        BlueStoreChecksumType::XxHash32 | BlueStoreChecksumType::Crc32c => 4,
        BlueStoreChecksumType::XxHash64 => 8,
        BlueStoreChecksumType::Crc32c16 => 2,
        BlueStoreChecksumType::Crc32c8 => 1,
    }
}

fn validate_checksum_length(
    raw_type: u8,
    word_size: usize,
    chunk_order: u8,
    encoded_length: usize,
    checksum_coverage_length: u32,
) -> Result<()> {
    if encoded_length == 0 {
        return Err(invalid_checksum(
            raw_type,
            "data length must be non-zero when checksum metadata is present",
        ));
    }
    if !encoded_length.is_multiple_of(word_size) {
        return Err(invalid_checksum(
            raw_type,
            "data length is not a checksum-word multiple",
        ));
    }
    let chunk_size = 1u64 << chunk_order;
    if u64::from(checksum_coverage_length) % chunk_size != 0 {
        return Err(invalid_checksum(
            raw_type,
            "checksum coverage length is not chunk aligned",
        ));
    }
    let expected_length = u64::from(checksum_coverage_length)
        .checked_div(chunk_size)
        .and_then(|count| count.checked_mul(word_size as u64))
        .ok_or(CephWireError::IntegerOverflow {
            context: "BlueStore checksum data length",
        })?;
    if encoded_length as u64 != expected_length {
        return Err(invalid_checksum(
            raw_type,
            "data length does not exactly match the on-disk chunk count",
        ));
    }
    Ok(())
}

fn decode_words(data: &[u8], word_size: usize) -> Vec<u64> {
    data.chunks_exact(word_size)
        .map(|word| match word_size {
            1 => u64::from(word[0]),
            2 => u64::from(u16::from_le_bytes([word[0], word[1]])),
            4 => u64::from(u32::from_le_bytes([word[0], word[1], word[2], word[3]])),
            8 => u64::from_le_bytes([
                word[0], word[1], word[2], word[3], word[4], word[5], word[6], word[7],
            ]),
            _ => unreachable!("checksum word size is fixed by the decoded checksum type"),
        })
        .collect()
}

fn invalid_checksum(checksum_type: u8, reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreChecksum {
        checksum_type,
        reason,
    }
}
