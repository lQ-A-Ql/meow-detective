#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    start: u64,
    length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ByteRangeError {
    #[error("byte range exceeds the address space")]
    Overflow,
    #[error("byte range exceeds target length {logical_size}")]
    OutOfBounds { logical_size: u64 },
}

impl ByteRange {
    pub fn new(start: u64, length: u64) -> Result<Self, ByteRangeError> {
        start.checked_add(length).ok_or(ByteRangeError::Overflow)?;
        Ok(Self { start, length })
    }

    pub fn validate(self, logical_size: u64) -> Result<(), ByteRangeError> {
        if self.end() > logical_size {
            return Err(ByteRangeError::OutOfBounds { logical_size });
        }
        Ok(())
    }

    pub fn start(self) -> u64 {
        self.start
    }

    pub fn length(self) -> u64 {
        self.length
    }

    pub fn end(self) -> u64 {
        self.start + self.length
    }
}
