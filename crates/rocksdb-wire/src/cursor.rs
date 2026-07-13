use crate::error::{Result, RocksDbWireError};

#[derive(Debug, Clone)]
pub(crate) struct WireCursor<'a> {
    input: &'a [u8],
    offset: usize,
    base_offset: usize,
}

impl<'a> WireCursor<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self::new_at(input, 0)
    }

    pub(crate) fn new_at(input: &'a [u8], base_offset: usize) -> Self {
        Self {
            input,
            offset: 0,
            base_offset,
        }
    }

    pub(crate) fn position(&self) -> usize {
        self.base_offset + self.offset
    }

    pub(crate) fn remaining(&self) -> usize {
        self.input.len() - self.offset
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub(crate) fn read_exact(&mut self, length: usize, context: &'static str) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(RocksDbWireError::LengthOverflow { context })?;
        if end > self.input.len() {
            return Err(RocksDbWireError::UnexpectedEof {
                offset: self.position(),
                context,
            });
        }
        let bytes = &self.input[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    pub(crate) fn read_u8(&mut self, context: &'static str) -> Result<u8> {
        Ok(self.read_exact(1, context)?[0])
    }

    pub(crate) fn read_fixed_u64(&mut self, context: &'static str) -> Result<u64> {
        let bytes = self.read_exact(8, context)?;
        Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
            RocksDbWireError::InvalidField {
                context,
                reason: "fixed64 width",
            }
        })?))
    }

    pub(crate) fn read_varint_u32(&mut self, context: &'static str) -> Result<u32> {
        let value = self.read_varint(context, 5, 0x0f)?;
        u32::try_from(value).map_err(|_| RocksDbWireError::VarintOverflow {
            offset: self.position(),
            context,
        })
    }

    pub(crate) fn read_varint_u64(&mut self, context: &'static str) -> Result<u64> {
        self.read_varint(context, 10, 0x01)
    }

    pub(crate) fn read_length_prefixed(
        &mut self,
        context: &'static str,
        limit: usize,
    ) -> Result<&'a [u8]> {
        let length = self.read_varint_u32(context)? as usize;
        if length > limit {
            return Err(RocksDbWireError::FieldLengthLimit {
                context,
                length,
                limit,
            });
        }
        self.read_exact(length, context)
    }

    fn read_varint(
        &mut self,
        context: &'static str,
        max_bytes: usize,
        final_byte_max: u8,
    ) -> Result<u64> {
        let start = self.position();
        let mut value = 0u64;
        for index in 0..max_bytes {
            let byte = self.read_u8(context)?;
            if index == max_bytes - 1 && byte > final_byte_max {
                return Err(RocksDbWireError::VarintOverflow {
                    offset: start,
                    context,
                });
            }
            value |= u64::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                if index > 0 && byte == 0 {
                    return Err(RocksDbWireError::NonCanonicalVarint {
                        offset: start,
                        context,
                    });
                }
                return Ok(value);
            }
        }
        Err(RocksDbWireError::VarintTooLong {
            offset: start,
            context,
            max_bytes,
        })
    }
}
