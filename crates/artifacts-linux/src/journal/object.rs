use std::io::{Cursor, Read};

use crate::LinuxArtifactError;

use super::header::{read_le_u64, read_u8, Header};

pub(super) const _OBJECT_UNUSED: u8 = 0;
pub(super) const OBJECT_DATA: u8 = 1;
pub(super) const OBJECT_ENTRY: u8 = 3;
pub(super) const _OBJECT_DATA_HASH_TABLE: u8 = 4;
pub(super) const _OBJECT_FIELD_HASH_TABLE: u8 = 5;
pub(super) const OBJECT_ENTRY_ARRAY: u8 = 6;
pub(super) const _OBJECT_TAG: u8 = 7;

#[derive(Debug)]
pub(super) struct ObjectHeader {
    pub(super) object_type: u8,
    pub(super) payload_size: u64,
}

impl ObjectHeader {
    pub(super) fn read(reader: &mut Cursor<&[u8]>) -> Result<Self, LinuxArtifactError> {
        let object_type = read_u8(reader)?;
        let _flags = read_u8(reader)?;
        let mut reserved = [0u8; 6];
        reader.read_exact(&mut reserved)?;
        let payload_size = read_le_u64(reader)?;
        Ok(Self {
            object_type,
            payload_size,
        })
    }
}

pub(super) fn collect_entry_offsets(
    reader: &mut Cursor<&[u8]>,
    data: &[u8],
    header: &Header,
) -> Vec<u64> {
    let object_start = reader.position();
    let mut offset = object_start;
    let mut entry_offsets = Vec::new();

    while offset < header.arena_size() && (offset as usize) < data.len() {
        reader.set_position(offset);
        let object = match ObjectHeader::read(reader) {
            Ok(header) => header,
            Err(_) => break,
        };

        let aligned_size = object.payload_size.div_ceil(8) * 8;
        let next_offset = offset + 16 + aligned_size;

        if object.payload_size == 0 {
            offset = next_offset;
            continue;
        }

        if object.object_type == OBJECT_ENTRY_ARRAY {
            let count = object.payload_size / 8;
            for _ in 0..count {
                if let Ok(entry_offset) = read_le_u64(reader) {
                    if entry_offset > 0 {
                        entry_offsets.push(entry_offset);
                    }
                }
            }
        }

        offset = next_offset;
    }

    if entry_offsets.is_empty() {
        offset = object_start;
        while offset < header.arena_size() && (offset as usize) < data.len() {
            reader.set_position(offset);
            let object = match ObjectHeader::read(reader) {
                Ok(header) => header,
                Err(_) => break,
            };
            let aligned_size = object.payload_size.div_ceil(8) * 8;
            let next_offset = offset + 16 + aligned_size;

            if object.object_type == OBJECT_ENTRY && object.payload_size > 0 {
                entry_offsets.push(offset);
            }

            offset = next_offset;
        }
    }

    entry_offsets
}

pub(super) fn decompress_if_needed(data: &[u8]) -> Option<Vec<u8>> {
    let is_lz4 = data.starts_with(&[0x02, 0x21, 0x4c, 0x18]);
    let is_zstd = data.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]);
    if is_lz4 || is_zstd {
        None
    } else {
        Some(data.to_vec())
    }
}
