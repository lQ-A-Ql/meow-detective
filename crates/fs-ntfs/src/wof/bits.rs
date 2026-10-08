use super::{Result, WofError};

/// Microsoft codecs use most-significant bits in little-endian 16-bit words.
pub(super) struct Bits<'a> {
    data: &'a [u8],
    position: usize,
    buffer: u64,
    available: u8,
    prefetch: bool,
}

impl<'a> Bits<'a> {
    pub(super) fn new(data: &'a [u8], prefetch: bool) -> Result<Self> {
        let mut bits = Self {
            data,
            position: 0,
            buffer: 0,
            available: 0,
            prefetch,
        };
        if prefetch {
            bits.feed()?;
            bits.feed()?;
        }
        Ok(bits)
    }

    fn feed(&mut self) -> Result<()> {
        let word = self.raw(2)?;
        self.buffer = (self.buffer << 16) | u64::from(u16::from_le_bytes([word[0], word[1]]));
        self.available += 16;
        Ok(())
    }

    pub(super) fn peek(&mut self, count: u8) -> Result<u32> {
        if count > 16 {
            return Err(WofError::Invalid("bit read exceeds codec word"));
        }
        while self.available < count {
            self.feed()?;
        }
        Ok(((self.buffer >> (self.available - count)) & ((1u64 << count) - 1)) as u32)
    }

    pub(super) fn read(&mut self, count: u8) -> Result<u32> {
        let value = self.peek(count)?;
        self.available -= count;
        self.buffer &= (1u64 << self.available) - 1;
        if self.prefetch
            && self.available < 16
            && self.data.len().saturating_sub(self.position) >= 2
        {
            self.feed()?;
        }
        Ok(value)
    }

    pub(super) fn align_uncompressed(&mut self) -> Result<()> {
        // Even an already aligned LZX block consumes one alignment word.
        if self.available == 0 {
            self.raw(2)?;
        }
        self.available = 0;
        self.buffer = 0;
        Ok(())
    }

    pub(super) fn raw(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(WofError::Invalid("input offset overflow"))?;
        let bytes = self
            .data
            .get(self.position..end)
            .ok_or(WofError::Invalid("truncated codec input"))?;
        self.position = end;
        Ok(bytes)
    }

    pub(super) fn raw_u8(&mut self) -> Result<u32> {
        Ok(u32::from(self.raw(1)?[0]))
    }

    pub(super) fn raw_u16(&mut self) -> Result<u32> {
        let data = self.raw(2)?;
        Ok(u32::from(u16::from_le_bytes([data[0], data[1]])))
    }

    pub(super) fn raw_u32(&mut self) -> Result<u32> {
        let data = self.raw(4)?;
        Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }
}
