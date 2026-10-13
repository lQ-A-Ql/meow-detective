use super::{BrowserError, HiveSnapshot};
use std::sync::Arc;

pub(super) fn u16_at(data: &[u8], at: usize) -> Result<u16, BrowserError> {
    Ok(u16::from_le_bytes(array(data, at)?))
}
pub(super) fn u32_at(data: &[u8], at: usize) -> Result<u32, BrowserError> {
    Ok(u32::from_le_bytes(array(data, at)?))
}
pub(super) fn array<const N: usize>(data: &[u8], at: usize) -> Result<[u8; N], BrowserError> {
    data.get(at..at.checked_add(N).ok_or(BrowserError::Corrupt)?)
        .and_then(|v| v.try_into().ok())
        .ok_or(BrowserError::Corrupt)
}

impl HiveSnapshot {
    pub fn open(bytes: Arc<Vec<u8>>) -> Result<Self, BrowserError> {
        if bytes.len() < 8192 || bytes.get(..4) != Some(b"regf") {
            return Err(BrowserError::Corrupt);
        }
        let major = u32_at(&bytes, 0x14)?;
        let minor = u32_at(&bytes, 0x18)?;
        if major != 1
            || !(3..=6).contains(&minor)
            || u32_at(&bytes, 0x1c)? != 0
            || u32_at(&bytes, 0x20)? != 1
        {
            return Err(BrowserError::Unsupported);
        }
        let length = u32_at(&bytes, 0x28)? as usize;
        let end = length
            .checked_add(4096)
            .filter(|end| *end <= bytes.len())
            .ok_or(BrowserError::Corrupt)?;
        if length == 0 || !length.is_multiple_of(4096) {
            return Err(BrowserError::Corrupt);
        }
        let root = u32_at(&bytes, 0x24)?;
        let dirty = u32_at(&bytes, 4)? != u32_at(&bytes, 8)?;
        let mut hive = Self {
            bytes,
            cells: vec![0; length.div_ceil(512)],
            root,
            end,
            minor,
            partial: false,
            dirty,
        };
        hive.index_cells()?;
        hive.key(root, String::new())?;
        Ok(hive)
    }

    fn index_cells(&mut self) -> Result<(), BrowserError> {
        let mut bin = 4096;
        while bin < self.end {
            if self.bytes.get(bin..bin + 4) != Some(b"hbin")
                || u32_at(&self.bytes, bin + 4)? as usize != bin - 4096
            {
                return Err(BrowserError::Corrupt);
            }
            let size = u32_at(&self.bytes, bin + 8)? as usize;
            let end = bin
                .checked_add(size)
                .filter(|end| *end <= self.end)
                .ok_or(BrowserError::Corrupt)?;
            if size < 4096 || !size.is_multiple_of(4096) {
                return Err(BrowserError::Corrupt);
            }
            let mut at = bin + 32;
            while at < end {
                let raw = i32::from_le_bytes(array(&self.bytes, at)?);
                let length = raw.checked_abs().ok_or(BrowserError::Corrupt)? as usize;
                if length < 8 || !length.is_multiple_of(8) || length > end - at {
                    self.partial = true;
                    break;
                }
                if raw < 0 {
                    let slot = (at - 4096) / 8;
                    self.cells[slot / 64] |= 1 << (slot % 64);
                }
                at += length;
            }
            bin = end;
        }
        Ok(())
    }

    pub(super) fn cell(&self, offset: u32) -> Result<&[u8], BrowserError> {
        let offset = offset as usize;
        if !offset.is_multiple_of(8) {
            return Err(BrowserError::Corrupt);
        }
        let slot = offset / 8;
        if self
            .cells
            .get(slot / 64)
            .is_none_or(|word| word & (1 << (slot % 64)) == 0)
        {
            return Err(BrowserError::Corrupt);
        }
        let at = 4096 + offset;
        let raw = i32::from_le_bytes(array(&self.bytes, at)?);
        let size = raw.checked_neg().ok_or(BrowserError::Corrupt)? as usize;
        self.bytes
            .get(at + 4..at + size)
            .ok_or(BrowserError::Corrupt)
    }

    pub fn retained_bytes(&self) -> usize {
        self.bytes.capacity() + self.cells.len() * 8
    }
    pub fn is_partial(&self) -> bool {
        self.partial
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

pub(super) fn name(data: &[u8], compressed: bool) -> Result<String, BrowserError> {
    if compressed {
        return Ok(data.iter().map(|v| char::from(*v)).collect());
    }
    if !data.len().is_multiple_of(2) {
        return Err(BrowserError::Corrupt);
    }
    String::from_utf16(
        &data
            .chunks_exact(2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect::<Vec<_>>(),
    )
    .map_err(|_| BrowserError::Corrupt)
}
