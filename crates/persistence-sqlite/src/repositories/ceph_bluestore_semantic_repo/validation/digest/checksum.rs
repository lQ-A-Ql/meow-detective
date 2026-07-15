use sha2::Digest;

use super::CanonicalDigest;
use crate::repositories::ceph_bluestore_semantic_repo::CephBluestoreChecksumChunkRecord;

pub(super) fn write_checksum_chunk(
    digest: &mut CanonicalDigest,
    inventory_id: &str,
    object_id: &str,
    record: &CephBluestoreChecksumChunkRecord,
) {
    let width_bytes = record.checksum_value_bytes;
    let checksum_length = usize::from(width_bytes) * 2;
    let required = 8usize
        .checked_add("checksum_chunk".len())
        .and_then(|value| value.checked_add(8 + inventory_id.len()))
        .and_then(|value| value.checked_add(8 + object_id.len()))
        .and_then(|value| value.checked_add(24 + 8 + checksum_length));
    let mut encoded = [0u8; 256];
    if !(1..=8).contains(&width_bytes) || required.is_none_or(|length| length > encoded.len()) {
        write_fallback(digest, inventory_id, object_id, record);
        return;
    }

    let mut cursor = 0usize;
    append_length_prefixed(&mut encoded, &mut cursor, b"checksum_chunk");
    append_length_prefixed(&mut encoded, &mut cursor, inventory_id.as_bytes());
    append_length_prefixed(&mut encoded, &mut cursor, object_id.as_bytes());
    append_fixed(
        &mut encoded,
        &mut cursor,
        &record.blob_ordinal.to_be_bytes(),
    );
    append_fixed(
        &mut encoded,
        &mut cursor,
        &record.checksum_ordinal.to_be_bytes(),
    );
    append_fixed(
        &mut encoded,
        &mut cursor,
        &record.chunk_offset.to_be_bytes(),
    );
    append_fixed(
        &mut encoded,
        &mut cursor,
        &record.chunk_length.to_be_bytes(),
    );
    append_fixed(
        &mut encoded,
        &mut cursor,
        &(checksum_length as u64).to_be_bytes(),
    );
    append_checksum_hex(
        &mut encoded,
        &mut cursor,
        record.checksum_value,
        checksum_length,
    );
    digest.hasher.update(&encoded[..cursor]);
}

fn write_fallback(
    digest: &mut CanonicalDigest,
    inventory_id: &str,
    object_id: &str,
    record: &CephBluestoreChecksumChunkRecord,
) {
    digest.tag("checksum_chunk");
    digest.text(inventory_id);
    digest.text(object_id);
    digest.u32(record.blob_ordinal);
    digest.u32(record.checksum_ordinal);
    digest.u64(record.chunk_offset);
    digest.u64(record.chunk_length);
    digest.checksum_hex(record.checksum_value, record.checksum_value_bytes);
}

fn append_length_prefixed(buffer: &mut [u8], cursor: &mut usize, value: &[u8]) {
    append_fixed(buffer, cursor, &(value.len() as u64).to_be_bytes());
    append_fixed(buffer, cursor, value);
}

fn append_fixed(buffer: &mut [u8], cursor: &mut usize, value: &[u8]) {
    let end = *cursor + value.len();
    buffer[*cursor..end].copy_from_slice(value);
    *cursor = end;
}

fn append_checksum_hex(buffer: &mut [u8], cursor: &mut usize, value: u64, length: usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let end = *cursor + length;
    for (index, output) in buffer[*cursor..end].iter_mut().enumerate() {
        let shift = (length - index - 1) * 4;
        *output = HEX[((value >> shift) & 0x0f) as usize];
    }
    *cursor = end;
}
