use evidence_core::{EvidenceReader, ReaderInfo};
use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use crate::build_segment_path;

const SEQUENTIAL_PREFETCH_CHUNKS: u64 = 3;
pub(crate) const SEQUENTIAL_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct CachedChunk {
    pub(crate) idx: u64,
    pub(crate) data: Arc<[u8]>,
}

/// E01 reader with multi-segment support.
///
/// The chunk table maps each entry to
/// `(segment_index, file_offset, compressed, stored_size)`.
pub struct E01Reader {
    pub(crate) info: ReaderInfo,
    pub(crate) total_bytes: u64,
    pub(crate) chunk_size_sectors: u32,
    pub(crate) chunk_table: Arc<Vec<(usize, u64, bool, u64)>>,
    pub(crate) segment_files: Vec<std::fs::File>,
    pub(crate) cursor: u64,
    pub(crate) chunk_cache: VecDeque<CachedChunk>,
    pub(crate) chunk_cache_bytes: usize,
    last_chunk_read: Option<u64>,
}

impl E01Reader {
    pub(crate) fn from_parts(
        info: ReaderInfo,
        total_bytes: u64,
        chunk_size_sectors: u32,
        chunk_table: Vec<(usize, u64, bool, u64)>,
        segment_files: Vec<std::fs::File>,
    ) -> Self {
        Self {
            info,
            total_bytes,
            chunk_size_sectors,
            chunk_table: Arc::new(chunk_table),
            segment_files,
            cursor: 0,
            chunk_cache: VecDeque::new(),
            chunk_cache_bytes: 0,
            last_chunk_read: None,
        }
    }

    /// Clone immutable parsing state while resetting cursor and cache state.
    pub fn try_clone(&self) -> io::Result<Self> {
        let segment_files: io::Result<Vec<_>> = self
            .segment_files
            .iter()
            .map(|file| file.try_clone())
            .collect();
        Ok(Self {
            info: self.info.clone(),
            total_bytes: self.total_bytes,
            chunk_size_sectors: self.chunk_size_sectors,
            chunk_table: Arc::clone(&self.chunk_table),
            segment_files: segment_files?,
            cursor: 0,
            chunk_cache: VecDeque::new(),
            chunk_cache_bytes: 0,
            last_chunk_read: None,
        })
    }

    /// Open independent segment handles while reusing the parsed chunk table.
    pub fn re_open(&self, source_path: &Path) -> io::Result<Self> {
        let mut segment_files = Vec::new();
        for segment in 1u32.. {
            match std::fs::File::open(build_segment_path(source_path, segment)) {
                Ok(file) => segment_files.push(file),
                Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                Err(error) => return Err(error),
            }
        }
        if segment_files.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no E01 segments found",
            ));
        }
        Ok(Self {
            info: self.info.clone(),
            total_bytes: self.total_bytes,
            chunk_size_sectors: self.chunk_size_sectors,
            chunk_table: Arc::clone(&self.chunk_table),
            segment_files,
            cursor: 0,
            chunk_cache: VecDeque::new(),
            chunk_cache_bytes: 0,
            last_chunk_read: None,
        })
    }

    fn read_chunk_uncached(&mut self, idx: u64) -> io::Result<Vec<u8>> {
        let (segment, offset, compressed, stored_size) = chunk_entry(&self.chunk_table, idx)?;
        let chunk_bytes = self.chunk_size_sectors as usize * 512;
        if segment >= self.segment_files.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "chunk references segment {} but only {} available",
                    segment,
                    self.segment_files.len()
                ),
            ));
        }

        let file = &mut self.segment_files[segment];
        file.seek(SeekFrom::Start(offset))?;
        if compressed {
            if stored_size == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "compressed chunk has zero stored size",
                ));
            }
            let mut raw = vec![0u8; stored_size as usize];
            file.read_exact(&mut raw)?;
            let mut decoder = flate2::read::ZlibDecoder::new(&raw[..]);
            let mut buffer = vec![0u8; chunk_bytes];
            decoder
                .read_exact(&mut buffer)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            return Ok(buffer);
        }

        let read_size = if stored_size == 0 {
            chunk_bytes
        } else {
            stored_size.min(chunk_bytes as u64) as usize
        };
        if read_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "uncompressed chunk has zero stored size",
            ));
        }
        let mut buffer = vec![0u8; chunk_bytes];
        file.read_exact(&mut buffer[..read_size])?;
        if read_size < chunk_bytes {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ));
        }
        Ok(buffer)
    }

    fn read_chunk_cached(&mut self, idx: u64, sequential: bool) -> io::Result<Arc<[u8]>> {
        if let Some(data) = self.cached_chunk(idx) {
            if sequential {
                self.prefetch_chunks_after(idx);
            }
            return Ok(data);
        }
        let data = Arc::<[u8]>::from(self.read_chunk_uncached(idx)?.into_boxed_slice());
        self.insert_cached_chunk(idx, Arc::clone(&data));
        if sequential {
            self.prefetch_chunks_after(idx);
        }
        Ok(data)
    }

    fn cached_chunk(&self, idx: u64) -> Option<Arc<[u8]>> {
        self.chunk_cache
            .iter()
            .find(|chunk| chunk.idx == idx)
            .map(|chunk| Arc::clone(&chunk.data))
    }

    fn insert_cached_chunk(&mut self, idx: u64, data: Arc<[u8]>) {
        if data.len() > SEQUENTIAL_CACHE_MAX_BYTES
            || self.chunk_cache.iter().any(|chunk| chunk.idx == idx)
        {
            return;
        }
        self.chunk_cache_bytes = self.chunk_cache_bytes.saturating_add(data.len());
        self.chunk_cache.push_back(CachedChunk { idx, data });
        while self.chunk_cache_bytes > SEQUENTIAL_CACHE_MAX_BYTES && self.chunk_cache.len() > 1 {
            if let Some(old) = self.chunk_cache.pop_front() {
                self.chunk_cache_bytes = self.chunk_cache_bytes.saturating_sub(old.data.len());
            }
        }
    }

    fn prefetch_chunks_after(&mut self, idx: u64) {
        let Some(start) = idx.checked_add(1) else {
            return;
        };
        for next in start..=idx.saturating_add(SEQUENTIAL_PREFETCH_CHUNKS) {
            if next as usize >= self.chunk_table.len() || self.cached_chunk(next).is_some() {
                continue;
            }
            match self.read_chunk_uncached(next) {
                Ok(data) => {
                    self.insert_cached_chunk(next, Arc::<[u8]>::from(data.into_boxed_slice()))
                }
                Err(error) => {
                    tracing::debug!("E01 sequential prefetch stopped at chunk {next}: {error}");
                    break;
                }
            }
        }
    }

    fn read_bytes(&mut self, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
        if offset >= self.total_bytes {
            return Ok(0);
        }
        let mut total = 0;
        let mut position = offset;
        let chunk_size = self.chunk_size_sectors as u64 * 512;
        while total < buffer.len() && position < self.total_bytes {
            let chunk_idx = position / chunk_size;
            let intra = (position % chunk_size) as usize;
            let sequential = self
                .last_chunk_read
                .is_some_and(|last| chunk_idx == last || chunk_idx == last + 1);
            let data = self.read_chunk_cached(chunk_idx, sequential)?;
            let available = (data.len() - intra).min(buffer.len() - total);
            buffer[total..total + available].copy_from_slice(&data[intra..intra + available]);
            total += available;
            position += available as u64;
            self.last_chunk_read = Some(chunk_idx);
        }
        Ok(total)
    }
}

impl Read for E01Reader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.read_bytes(buffer, self.cursor)?;
        self.cursor += read as u64;
        Ok(read)
    }
}

impl Seek for E01Reader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.cursor = match position {
            SeekFrom::Start(position) => position.min(self.total_bytes),
            SeekFrom::End(delta) => ((self.total_bytes as i64) + delta).max(0) as u64,
            SeekFrom::Current(delta) => ((self.cursor as i64) + delta).max(0) as u64,
        }
        .min(self.total_bytes);
        self.last_chunk_read = None;
        Ok(self.cursor)
    }
}

impl EvidenceReader for E01Reader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn chunk_entry(table: &[(usize, u64, bool, u64)], idx: u64) -> io::Result<(usize, u64, bool, u64)> {
    table
        .get(idx as usize)
        .copied()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk not found"))
}
