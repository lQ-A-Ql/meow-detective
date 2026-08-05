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
    pub(crate) bytes_per_sector: u32,
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
        bytes_per_sector: u32,
        chunk_table: Vec<(usize, u64, bool, u64)>,
        segment_files: Vec<std::fs::File>,
    ) -> Self {
        Self {
            info,
            total_bytes,
            chunk_size_sectors,
            bytes_per_sector,
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
            bytes_per_sector: self.bytes_per_sector,
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
            bytes_per_sector: self.bytes_per_sector,
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
        let chunk_bytes = usize::try_from(self.chunk_size_bytes()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "E01 chunk size does not fit the current platform",
            )
        })?;
        let context = ChunkReadContext {
            index: idx,
            segment,
            offset,
            stored_size,
            expected_size: chunk_bytes,
            codec: if compressed { "deflate" } else { "raw" },
        };
        if segment >= self.segment_files.len() {
            return Err(context.error(
                io::ErrorKind::InvalidData,
                format!(
                    "segment is unavailable; only {} segment(s) are open",
                    self.segment_files.len()
                ),
            ));
        }

        let file = &mut self.segment_files[segment];
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| context.source_error("seek source chunk", error))?;
        if compressed {
            return read_compressed_chunk(file, &context);
        }
        read_uncompressed_chunk(file, &context)
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
        let chunk_size = self.chunk_size_bytes();
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

    pub(crate) fn chunk_size_bytes(&self) -> u64 {
        u64::from(self.chunk_size_sectors) * u64::from(self.bytes_per_sector)
    }
}

struct ChunkReadContext {
    index: u64,
    segment: usize,
    offset: u64,
    stored_size: u64,
    expected_size: usize,
    codec: &'static str,
}

impl ChunkReadContext {
    fn error(&self, kind: io::ErrorKind, detail: impl std::fmt::Display) -> io::Error {
        io::Error::new(kind, format!("{}: {detail}", self.description()))
    }

    fn source_error(&self, operation: &str, error: io::Error) -> io::Error {
        self.error(error.kind(), format!("{operation} failed: {error}"))
    }

    fn description(&self) -> String {
        format!(
            "E01 chunk {} codec={} segment={} offset={} stored_length={} expected_decompressed_length={}",
            self.index,
            self.codec,
            self.segment,
            self.offset,
            self.stored_size,
            self.expected_size
        )
    }
}

fn read_compressed_chunk(
    file: &mut std::fs::File,
    context: &ChunkReadContext,
) -> io::Result<Vec<u8>> {
    if context.stored_size == 0 {
        return Err(context.error(
            io::ErrorKind::UnexpectedEof,
            "compressed chunk has zero stored length",
        ));
    }
    let raw = read_stored_bytes(file, context, context.stored_size)?;
    let decoder = flate2::read::ZlibDecoder::new(&raw[..]);
    let limit = u64::try_from(context.expected_size)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(context.expected_size.saturating_add(1))
        .map_err(|error| context.error(io::ErrorKind::OutOfMemory, error))?;
    decoder
        .take(limit)
        .read_to_end(&mut decoded)
        .map_err(|error| context.source_error("deflate decode", error))?;
    if decoded.len() != context.expected_size {
        return Err(context.error(
            io::ErrorKind::InvalidData,
            format!("deflate output length was {}", decoded.len()),
        ));
    }
    Ok(decoded)
}

fn read_uncompressed_chunk(
    file: &mut std::fs::File,
    context: &ChunkReadContext,
) -> io::Result<Vec<u8>> {
    if context.stored_size > 0 && context.stored_size < context.expected_size as u64 {
        return Err(context.error(
            io::ErrorKind::UnexpectedEof,
            "raw stored length is shorter than the expected chunk length",
        ));
    }
    read_stored_bytes(file, context, context.expected_size as u64)
}

fn read_stored_bytes(
    file: &mut std::fs::File,
    context: &ChunkReadContext,
    length: u64,
) -> io::Result<Vec<u8>> {
    let length = usize::try_from(length).map_err(|_| {
        context.error(
            io::ErrorKind::InvalidData,
            "stored length does not fit the current platform",
        )
    })?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|error| context.error(io::ErrorKind::OutOfMemory, error))?;
    bytes.resize(length, 0);
    file.read_exact(&mut bytes)
        .map_err(|error| context.source_error("read source chunk", error))?;
    Ok(bytes)
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
