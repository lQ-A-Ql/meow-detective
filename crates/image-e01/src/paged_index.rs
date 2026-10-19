//! Immutable section index and bounded, shared offset-page cache.
use crate::{checksum::page_digest, sections::invalid};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    sync::{Arc, Mutex},
};

pub(crate) const PAGE_BYTES: usize = 64 * 1024;
pub(crate) const CACHE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const PAGE_ENTRIES: u64 = (PAGE_BYTES / 4) as u64;
pub(crate) type ChunkEntry = (usize, u64, bool, u64);

pub(crate) struct TableSpan {
    pub first_chunk: u64,
    pub count: u64,
    pub segment: usize,
    pub base: u64,
    pub entries_offset: u64,
    pub last_end: u64,
    pub max_stored: u64,
    pub mirrored: bool,
    pub page_checksums: Vec<[u8; 32]>,
}

#[cfg(test)]
#[path = "../tests/unit/paged_index.rs"]
mod tests;

struct CachedPage {
    table: usize,
    page: u64,
    bytes: Arc<[u8]>,
}

pub(crate) struct PagedIndex {
    tables: Vec<TableSpan>,
    chunks: usize,
    cache_limit_pages: usize,
    cache: Mutex<VecDeque<CachedPage>>,
}

impl PagedIndex {
    pub fn new(tables: Vec<TableSpan>, expected: u64) -> io::Result<Self> {
        let chunks = tables.iter().try_fold(0u64, |total, table| {
            total
                .checked_add(table.count)
                .ok_or_else(|| invalid("E01 chunk count overflow"))
        })?;
        if chunks != expected {
            return Err(invalid(format!(
                "E01 chunk table count mismatch: expected {expected}, found {chunks}"
            )));
        }
        let cache_limit_pages = tables
            .iter()
            .map(|table| table.page_checksums.len())
            .sum::<usize>()
            .min(CACHE_BYTES / PAGE_BYTES);
        let index = Self {
            tables,
            chunks: usize::try_from(chunks).map_err(|_| crate::open_limits::limit())?,
            cache: Mutex::new(VecDeque::new()),
            cache_limit_pages,
        };
        if index.metadata_bytes() > crate::open_limits::MAX_METADATA_BYTES {
            return Err(crate::open_limits::limit());
        }
        Ok(index)
    }
    pub fn len(&self) -> usize {
        self.chunks
    }
    pub fn metadata_bytes(&self) -> usize {
        // Reserve the full cache before readers can allocate pages. Two in-flight
        // pages per reader are separately included in reader_memory_bound().
        self.cache_limit_pages * PAGE_BYTES
            + self.tables.capacity() * std::mem::size_of::<TableSpan>()
            + self
                .tables
                .iter()
                .map(|t| t.page_checksums.capacity() * 32)
                .sum::<usize>()
            + self.cache_limit_pages * (std::mem::size_of::<CachedPage>() + 32)
    }
    pub fn max_stored_chunk(&self) -> u64 {
        self.tables.iter().map(|t| t.max_stored).max().unwrap_or(0)
    }
    pub fn mirrored_tables(&self) -> usize {
        self.tables.iter().filter(|t| t.mirrored).count()
    }
    pub fn lookup(&self, index: u64, files: &mut [File]) -> io::Result<ChunkEntry> {
        if index >= self.chunks as u64 {
            return Err(invalid("E01 chunk index is out of range"));
        }
        let slot = self
            .tables
            .partition_point(|table| table.first_chunk <= index)
            - 1;
        let table = &self.tables[slot];
        let local = index - table.first_chunk;
        let page_number = local / PAGE_ENTRIES;
        let page = self.page(slot, page_number, files)?;
        let intra = local % PAGE_ENTRIES;
        let raw = raw_offset(&page, intra)?;
        let absolute = table
            .base
            .checked_add(u64::from(raw & 0x7fff_ffff))
            .ok_or_else(|| invalid("E01 chunk offset overflow"))?;
        let end = if local + 1 < table.count {
            let next = if intra + 1 < PAGE_ENTRIES {
                raw_offset(&page, intra + 1)?
            } else {
                raw_offset(&self.page(slot, page_number + 1, files)?, 0)?
            };
            table
                .base
                .checked_add(u64::from(next & 0x7fff_ffff))
                .ok_or_else(|| invalid("E01 chunk end overflow"))?
        } else {
            table.last_end
        };
        let length = end
            .checked_sub(absolute)
            .filter(|n| *n > 0 && *n <= crate::geometry::MAX_STORED_CHUNK_BYTES)
            .ok_or_else(|| invalid("E01 table page contains an invalid chunk range"))?;
        Ok((table.segment, absolute, raw & 0x8000_0000 != 0, length))
    }
    fn page(&self, slot: usize, page: u64, files: &mut [File]) -> io::Result<Arc<[u8]>> {
        let cached = {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| invalid("E01 table cache lock poisoned"))?;
            let found = cache
                .iter()
                .rposition(|entry| entry.table == slot && entry.page == page);
            found.and_then(|i| cache.remove(i)).map(|entry| {
                let bytes = Arc::clone(&entry.bytes);
                cache.push_back(entry);
                bytes
            })
        };
        Ok(match cached {
            Some(bytes) => bytes,
            None => self.load_page(slot, page, files)?,
        })
    }
    fn load_page(&self, slot: usize, page: u64, files: &mut [File]) -> io::Result<Arc<[u8]>> {
        let table = &self.tables[slot];
        let count = (table.count - page * PAGE_ENTRIES).min(PAGE_ENTRIES);
        let file = files
            .get_mut(table.segment)
            .ok_or_else(|| invalid("E01 table segment is missing"))?;
        file.seek(SeekFrom::Start(
            table.entries_offset + page * PAGE_BYTES as u64,
        ))?;
        let mut bytes = vec![0; count as usize * 4];
        file.read_exact(&mut bytes)?;
        if table.page_checksums.get(page as usize) != Some(&page_digest(&bytes)) {
            return Err(invalid("E01 table page changed since planning"));
        }
        let bytes: Arc<[u8]> = bytes.into();
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| invalid("E01 table cache lock poisoned"))?;
        if !cache
            .iter()
            .any(|entry| entry.table == slot && entry.page == page)
        {
            while cache.len() >= self.cache_limit_pages {
                cache.pop_front();
            }
            cache.push_back(CachedPage {
                table: slot,
                page,
                bytes: Arc::clone(&bytes),
            });
        }
        Ok(bytes)
    }
}

fn raw_offset(bytes: &[u8], intra: u64) -> io::Result<u32> {
    let offset = intra as usize * 4;
    bytes
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| invalid("E01 offset page is truncated"))
}
