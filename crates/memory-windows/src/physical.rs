use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use crate::{MemoryWindowsError, Result};

pub const PAGE_SIZE: usize = 0x1000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicalReadStats {
    pub operations: u64,
    pub bytes_read: u64,
    pub furthest_read_end: u64,
}

/// A read-only raw physical-memory image.
///
/// Reads are intentionally bounded and serialized through one file handle. This
/// avoids mapping a multi-gigabyte memory image into the application process.
pub struct RawMemoryImage {
    file: File,
    length: u64,
    read_stats: PhysicalReadStats,
    maximum_read_operations: Option<u64>,
    maximum_read_bytes: Option<u64>,
}

impl RawMemoryImage {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .map_err(|source| MemoryWindowsError::PhysicalRead { offset: 0, source })?;
        let length = file
            .metadata()
            .map_err(|source| MemoryWindowsError::PhysicalRead { offset: 0, source })?
            .len();
        if length == 0 {
            return Err(MemoryWindowsError::EmptyImage);
        }
        Ok(Self {
            file,
            length,
            read_stats: PhysicalReadStats::default(),
            maximum_read_operations: None,
            maximum_read_bytes: None,
        })
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        self.length
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    #[must_use]
    pub fn read_stats(&self) -> PhysicalReadStats {
        self.read_stats
    }

    pub(crate) fn set_read_budget(
        &mut self,
        maximum_operations: u64,
        maximum_bytes: u64,
    ) -> Result<()> {
        if maximum_operations == 0 || maximum_bytes == 0 {
            return Err(MemoryWindowsError::InvalidTargetedScanLimit {
                reason: "physical read budget must be non-zero",
            });
        }
        self.maximum_read_operations = Some(maximum_operations);
        self.maximum_read_bytes = Some(maximum_bytes);
        Ok(())
    }

    pub fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        let end = offset.checked_add(buffer.len() as u64).ok_or(
            MemoryWindowsError::PhysicalOutOfBounds {
                offset,
                end: u64::MAX,
                length: self.length,
            },
        )?;
        if end > self.length {
            return Err(MemoryWindowsError::PhysicalOutOfBounds {
                offset,
                end,
                length: self.length,
            });
        }
        let next_operations = self.read_stats.operations.checked_add(1).ok_or(
            MemoryWindowsError::TargetedScanBudgetExceeded {
                resource: "physical-read-operation",
                limit: self.maximum_read_operations.unwrap_or(u64::MAX),
            },
        )?;
        let next_bytes = self
            .read_stats
            .bytes_read
            .checked_add(buffer.len() as u64)
            .ok_or(MemoryWindowsError::TargetedScanBudgetExceeded {
                resource: "physical-read-byte",
                limit: self.maximum_read_bytes.unwrap_or(u64::MAX),
            })?;
        enforce_read_budget(
            next_operations,
            self.maximum_read_operations,
            "physical-read-operation",
        )?;
        enforce_read_budget(next_bytes, self.maximum_read_bytes, "physical-read-byte")?;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|source| MemoryWindowsError::PhysicalRead { offset, source })?;
        self.file
            .read_exact(buffer)
            .map_err(|source| MemoryWindowsError::PhysicalRead { offset, source })?;
        self.read_stats.operations = self.read_stats.operations.saturating_add(1);
        self.read_stats.bytes_read = self
            .read_stats
            .bytes_read
            .saturating_add(buffer.len() as u64);
        self.read_stats.furthest_read_end = self.read_stats.furthest_read_end.max(end);
        Ok(())
    }

    pub fn read_page(&mut self, physical_address: u64) -> Result<[u8; PAGE_SIZE]> {
        let mut page = [0u8; PAGE_SIZE];
        self.read_exact_at(physical_address, &mut page)?;
        Ok(page)
    }

    pub fn read_u64(&mut self, physical_address: u64) -> Result<u64> {
        let mut bytes = [0u8; 8];
        self.read_exact_at(physical_address, &mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }
}

fn enforce_read_budget(next: u64, maximum: Option<u64>, resource: &'static str) -> Result<()> {
    match maximum {
        Some(limit) if next > limit => {
            Err(MemoryWindowsError::TargetedScanBudgetExceeded { resource, limit })
        }
        _ => Ok(()),
    }
}
