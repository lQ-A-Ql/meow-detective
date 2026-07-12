use std::io::{Cursor, Read};

use crate::LinuxArtifactError;

pub(super) const JOURNAL_HEADER_SIGNATURE: &[u8; 8] = b"LPKSHHRH";
pub(super) const HEADER_INCOMPATIBLE_COMPRESSED: u32 = 0x04;

#[derive(Debug, Clone)]
pub(super) struct Header {
    incompatible_flags: u32,
    arena_size: u64,
}

impl Header {
    pub(super) fn read(reader: &mut Cursor<&[u8]>) -> Result<Self, LinuxArtifactError> {
        let position = |cursor: &mut Cursor<&[u8]>| cursor.position();

        let mut signature = [0u8; 8];
        reader.read_exact(&mut signature)?;
        if &signature != JOURNAL_HEADER_SIGNATURE {
            return Err(LinuxArtifactError::ParseError {
                parser: "journal",
                message: "Not a systemd journal file (invalid signature)".to_string(),
            });
        }

        let _compatible_flags = read_le_u32(reader)?;
        let incompatible_flags = read_le_u32(reader)?;
        let _state = read_u8(reader)?;
        let mut reserved = [0u8; 7];
        reader.read_exact(&mut reserved)?;

        let mut file_id = [0u8; 16];
        reader.read_exact(&mut file_id)?;
        let mut machine_id = [0u8; 16];
        reader.read_exact(&mut machine_id)?;
        let mut boot_id = [0u8; 16];
        reader.read_exact(&mut boot_id)?;
        let mut seqnum_id = [0u8; 16];
        reader.read_exact(&mut seqnum_id)?;

        let header_size = read_le_u64(reader)?;
        let arena_size = read_le_u64(reader)?;
        let _data_hash_table_offset = read_le_u64(reader)?;
        let _data_hash_table_size = read_le_u64(reader)?;
        let _field_hash_table_offset = read_le_u64(reader)?;
        let _field_hash_table_size = read_le_u64(reader)?;
        let _tail_object_offset = read_le_u64(reader)?;
        let _n_objects = read_le_u64(reader)?;
        let _n_entries = read_le_u64(reader)?;
        let _tail_entry_seqnum = read_le_u64(reader)?;
        let _head_entry_seqnum = read_le_u64(reader)?;
        let _entry_array_offset = read_le_u64(reader)?;
        let _head_entry_realtime = read_le_u64(reader)?;
        let _tail_entry_realtime = read_le_u64(reader)?;
        let _tail_entry_monotonic = read_le_u64(reader)?;

        if header_size > position(reader) {
            let remaining = (header_size - position(reader)) as usize;
            if reader.position() as usize + remaining > reader.get_ref().len() {
                return Err(LinuxArtifactError::ParseError {
                    parser: "journal",
                    message: "Header size exceeds file length".to_string(),
                });
            }
            reader.set_position(header_size);
        }

        let _n_data = read_le_u64(reader)?;
        let _n_fields = read_le_u64(reader)?;
        let _n_tags = read_le_u64(reader)?;
        let _n_entry_arrays = read_le_u64(reader)?;
        let _data_hash_chain_depth = read_le_u64(reader)?;
        let _field_hash_chain_depth = read_le_u64(reader)?;

        Ok(Self {
            incompatible_flags,
            arena_size,
        })
    }

    pub(super) fn incompatible_flags(&self) -> u32 {
        self.incompatible_flags
    }

    pub(super) fn arena_size(&self) -> u64 {
        self.arena_size
    }
}

pub(super) fn read_u8(reader: &mut Cursor<&[u8]>) -> Result<u8, LinuxArtifactError> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

pub(super) fn read_le_u32(reader: &mut Cursor<&[u8]>) -> Result<u32, LinuxArtifactError> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

pub(super) fn read_le_u64(reader: &mut Cursor<&[u8]>) -> Result<u64, LinuxArtifactError> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}
