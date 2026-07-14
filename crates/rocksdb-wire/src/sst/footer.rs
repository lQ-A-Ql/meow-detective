use crate::cursor::WireCursor;
use crate::{Result, RocksDbWireError};

use super::model::ChecksumType;
use super::BlockHandle;

pub const FOOTER_LENGTH: usize = 53;
pub const BLOCK_BASED_TABLE_MAGIC: u64 = 0x88e2_41b7_85f4_cff7;
const HANDLE_AREA_LENGTH: usize = 40;
const FORMAT_VERSION: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub checksum_type: ChecksumType,
    pub metaindex_handle: BlockHandle,
    pub index_handle: BlockHandle,
    pub format_version: u32,
    pub table_magic: u64,
}

impl Footer {
    pub(crate) fn decode(input: &[u8], file_size: u64) -> Result<Self> {
        if input.len() != FOOTER_LENGTH {
            return Err(RocksDbWireError::InvalidField {
                context: "SST footer",
                reason: "footer must be exactly 53 bytes",
            });
        }
        let footer_offset = file_size.checked_sub(FOOTER_LENGTH as u64).ok_or(
            RocksDbWireError::SstFileTooShort {
                file_size,
                minimum: FOOTER_LENGTH as u64,
            },
        )?;
        let checksum_type = decode_checksum(input[0])?;
        let mut cursor = WireCursor::new(&input[1..1 + HANDLE_AREA_LENGTH]);
        let metaindex_handle = BlockHandle::decode(&mut cursor, "SST metaindex handle")?;
        let index_handle = BlockHandle::decode(&mut cursor, "SST index handle")?;
        validate_padding(&input[1 + cursor.position()..1 + HANDLE_AREA_LENGTH])?;
        let format_version = u32::from_le_bytes(input[41..45].try_into().map_err(|_| {
            RocksDbWireError::InvalidField {
                context: "SST footer format version",
                reason: "fixed32 width",
            }
        })?);
        if format_version != FORMAT_VERSION {
            return Err(RocksDbWireError::UnsupportedSstFormatVersion {
                version: format_version,
            });
        }
        let table_magic = u64::from_le_bytes(input[45..53].try_into().map_err(|_| {
            RocksDbWireError::InvalidField {
                context: "SST footer table magic",
                reason: "fixed64 width",
            }
        })?);
        if table_magic != BLOCK_BASED_TABLE_MAGIC {
            return Err(RocksDbWireError::UnsupportedSstMagic { magic: table_magic });
        }
        metaindex_handle.validate_before(footer_offset)?;
        index_handle.validate_before(footer_offset)?;
        ensure_non_overlapping(metaindex_handle, index_handle)?;
        Ok(Self {
            checksum_type,
            metaindex_handle,
            index_handle,
            format_version,
            table_magic,
        })
    }
}

fn decode_checksum(value: u8) -> Result<ChecksumType> {
    if value == ChecksumType::XXH3_ID {
        Ok(ChecksumType::Xxh3)
    } else {
        Err(RocksDbWireError::UnsupportedSstChecksum {
            checksum_type: value,
        })
    }
}

fn validate_padding(padding: &[u8]) -> Result<()> {
    if let Some(offset) = padding.iter().position(|byte| *byte != 0) {
        return Err(RocksDbWireError::NonZeroSstFooterPadding { offset });
    }
    Ok(())
}

fn ensure_non_overlapping(first: BlockHandle, second: BlockHandle) -> Result<()> {
    let first_end = first.serialized_end()?;
    let second_end = second.serialized_end()?;
    if first.offset < second_end && second.offset < first_end {
        return Err(RocksDbWireError::InvalidBlockHandle {
            context: "SST footer handles",
            reason: "metaindex and index blocks overlap",
        });
    }
    Ok(())
}
