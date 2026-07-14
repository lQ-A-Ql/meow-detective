use crate::cursor::WireCursor;
use crate::{Result, RocksDbWireError};

use super::model::BLOCK_TRAILER_LENGTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHandle {
    pub offset: u64,
    pub size: u64,
}

impl BlockHandle {
    pub(crate) fn decode(cursor: &mut WireCursor<'_>, context: &'static str) -> Result<Self> {
        let offset = cursor.read_varint_u64(context)?;
        let size = cursor.read_varint_u64(context)?;
        if size == 0 {
            return Err(RocksDbWireError::InvalidBlockHandle {
                context,
                reason: "zero-sized block",
            });
        }
        Ok(Self { offset, size })
    }

    pub(crate) fn serialized_end(self) -> Result<u64> {
        self.offset
            .checked_add(self.size)
            .and_then(|end| end.checked_add(BLOCK_TRAILER_LENGTH as u64))
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "SST block range",
            })
    }

    pub(crate) fn validate_before(self, boundary: u64) -> Result<()> {
        let end = self.serialized_end()?;
        if end > boundary {
            return Err(RocksDbWireError::SstBlockOutOfRange {
                offset: self.offset,
                end,
                boundary,
            });
        }
        Ok(())
    }
}
