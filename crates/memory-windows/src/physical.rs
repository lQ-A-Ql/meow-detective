use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use crate::{MemoryWindowsError, Result};

pub const PAGE_SIZE: usize = 0x1000;
const SCAN_CHUNK_SIZE: usize = 1024 * 1024;

/// A read-only raw physical-memory image.
///
/// Reads are intentionally bounded and serialized through one file handle. This
/// avoids mapping a multi-gigabyte memory image into the application process.
pub struct RawMemoryImage {
    file: File,
    length: u64,
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
        Ok(Self { file, length })
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        self.length
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.length == 0
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
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|source| MemoryWindowsError::PhysicalRead { offset, source })?;
        self.file
            .read_exact(buffer)
            .map_err(|source| MemoryWindowsError::PhysicalRead { offset, source })
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

    pub(crate) fn visit_pages<F>(&mut self, mut visitor: F) -> Result<()>
    where
        F: FnMut(u64, &[u8]) -> bool,
    {
        let mut buffer = vec![0u8; SCAN_CHUNK_SIZE];
        let mut offset = 0u64;
        while offset + PAGE_SIZE as u64 <= self.length {
            let remaining = (self.length - offset) as usize;
            let take = remaining.min(buffer.len() / PAGE_SIZE * PAGE_SIZE);
            self.read_exact_at(offset, &mut buffer[..take])?;
            for (index, page) in buffer[..take].chunks_exact(PAGE_SIZE).enumerate() {
                if !visitor(offset + (index * PAGE_SIZE) as u64, page) {
                    return Ok(());
                }
            }
            offset += take as u64;
        }
        Ok(())
    }

    /// Locates exact four-byte structure tags without retaining memory contents.
    pub fn scan_tag(&mut self, tag: [u8; 4], maximum_matches: usize) -> Result<Vec<u64>> {
        self.scan_bytes(&tag, maximum_matches)
    }

    /// Locates several four-byte tags in one physical pass. The returned index
    /// identifies the matching element in `tags`.
    pub(crate) fn scan_tags(
        &mut self,
        tags: &[[u8; 4]],
        maximum_matches_per_tag: usize,
    ) -> Result<Vec<(usize, u64)>> {
        if tags.is_empty() || maximum_matches_per_tag == 0 {
            return Ok(Vec::new());
        }
        let mut matches = Vec::new();
        let mut counts = vec![0usize; tags.len()];
        let mut buffer = vec![0u8; SCAN_CHUNK_SIZE];
        let mut offset = 0u64;
        while offset < self.length && counts.iter().any(|count| *count < maximum_matches_per_tag) {
            let take = ((self.length - offset) as usize).min(buffer.len());
            self.read_exact_at(offset, &mut buffer[..take])?;
            if take >= 4 {
                let searchable = &buffer[..take - 3];
                for (tag_index, tag) in tags.iter().enumerate() {
                    if counts[tag_index] >= maximum_matches_per_tag {
                        continue;
                    }
                    for start in memchr::memchr_iter(tag[0], searchable) {
                        if counts[tag_index] == maximum_matches_per_tag {
                            break;
                        }
                        if buffer[start..start + 4] == *tag {
                            matches.push((tag_index, offset + start as u64));
                            counts[tag_index] += 1;
                        }
                    }
                }
            }
            if take < 4 {
                break;
            }
            offset = offset.saturating_add((take - 3) as u64);
        }
        Ok(matches)
    }

    /// Locates an exact structural marker with a bounded Boyer-Moore-Horspool scan.
    pub fn scan_bytes(&mut self, pattern: &[u8], maximum_matches: usize) -> Result<Vec<u64>> {
        if pattern.is_empty() || maximum_matches == 0 {
            return Ok(Vec::new());
        }
        let mut matches = Vec::new();
        let mut buffer = vec![0u8; SCAN_CHUNK_SIZE];
        let mut shifts = [pattern.len(); 256];
        for (index, byte) in pattern[..pattern.len() - 1].iter().enumerate() {
            shifts[*byte as usize] = pattern.len() - 1 - index;
        }
        let mut offset = 0u64;
        while offset < self.length && matches.len() < maximum_matches {
            let remaining = (self.length - offset) as usize;
            let take = remaining.min(buffer.len());
            self.read_exact_at(offset, &mut buffer[..take])?;
            if take >= pattern.len() {
                let mut end = pattern.len() - 1;
                while end < take && matches.len() < maximum_matches {
                    let start = end + 1 - pattern.len();
                    if buffer[end] == pattern[pattern.len() - 1] && buffer[start..=end] == *pattern
                    {
                        matches.push(offset + start as u64);
                    }
                    end = end.saturating_add(shifts[buffer[end] as usize].max(1));
                }
            }
            if take < pattern.len() {
                break;
            }
            offset = offset.saturating_add((take - (pattern.len() - 1)) as u64);
        }
        Ok(matches)
    }
}
