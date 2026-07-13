use crate::error::{CephWireError, Result};

/// A bounded cursor over an immutable Ceph-encoded byte slice.
#[derive(Debug, Clone)]
pub struct CephCursor<'a> {
    input: &'a [u8],
    offset: usize,
    limit: usize,
}

impl<'a> CephCursor<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            offset: 0,
            limit: input.len(),
        }
    }

    pub fn position(&self) -> usize {
        self.offset
    }

    pub fn remaining(&self) -> usize {
        self.limit - self.offset
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn input(&self) -> &'a [u8] {
        self.input
    }

    pub fn read_exact(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(CephWireError::LengthOverflow { context: "cursor" })?;
        if end > self.limit {
            return Err(CephWireError::UnexpectedEof {
                offset: self.offset,
                needed: length,
                remaining: self.remaining(),
            });
        }
        let bytes = &self.input[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    pub fn skip(&mut self, length: usize) -> Result<()> {
        self.read_exact(length).map(|_| ())
    }

    pub fn take(&mut self, length: usize) -> Result<Self> {
        let bytes = self.read_exact(length)?;
        Ok(Self::new(bytes))
    }
}
