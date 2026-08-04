use crate::BlockDeviceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockGeometry {
    byte_len: u64,
    block_size: u32,
    block_count: u64,
}

impl BlockGeometry {
    pub fn new(byte_len: u64, block_size: u32) -> Result<Self, BlockDeviceError> {
        if block_size == 0 || !block_size.is_power_of_two() {
            return Err(BlockDeviceError::InvalidBlockSize);
        }
        if byte_len == 0 {
            return Err(BlockDeviceError::EmptyImage);
        }
        if !byte_len.is_multiple_of(u64::from(block_size)) {
            return Err(BlockDeviceError::UnalignedImageSize {
                size: byte_len,
                block_size,
            });
        }
        Ok(Self {
            byte_len,
            block_size,
            block_count: byte_len / u64::from(block_size),
        })
    }

    pub fn byte_len(self) -> u64 {
        self.byte_len
    }

    pub fn block_size(self) -> u32 {
        self.block_size
    }

    pub fn block_count(self) -> u64 {
        self.block_count
    }

    pub fn byte_range(self, lba: u64, blocks: u32) -> Result<(u64, u64), BlockDeviceError> {
        let offset = lba
            .checked_mul(u64::from(self.block_size))
            .ok_or(BlockDeviceError::ArithmeticOverflow)?;
        let length = u64::from(blocks)
            .checked_mul(u64::from(self.block_size))
            .ok_or(BlockDeviceError::ArithmeticOverflow)?;
        let end = offset
            .checked_add(length)
            .ok_or(BlockDeviceError::ArithmeticOverflow)?;
        if end > self.byte_len {
            return Err(BlockDeviceError::OutOfBounds {
                offset,
                end,
                size: self.byte_len,
            });
        }
        Ok((offset, length))
    }
}
