use evidence_core::{EvidenceReader, ReaderInfo};
use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod open;

const SECTION_DESCRIPTOR_SIZE: u64 = 76;
const V1_TABLE_HEADER_SIZE: usize = 24;
const SEQUENTIAL_PREFETCH_CHUNKS: u64 = 3;
const SEQUENTIAL_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

struct CachedChunk {
    idx: u64,
    data: Arc<[u8]>,
}

/// E01 reader with multi-segment support.
/// Opens .E01 and auto-detects .E02, .E03... files.
/// Chunk table maps each entry to (segment_index, file_offset, compressed, stored_size).
pub struct E01Reader {
    info: ReaderInfo,
    total_bytes: u64,
    chunk_size_sectors: u32,
    chunk_table: Arc<Vec<(usize, u64, bool, u64)>>, // (segment, offset, compressed, stored_size)
    segment_files: Vec<std::fs::File>,
    cursor: u64,
    chunk_cache: VecDeque<CachedChunk>,
    chunk_cache_bytes: usize,
    last_chunk_read: Option<u64>,
}

/// Clone the reader's immutable parsing state (metadata, chunk table, segment
/// file handles) but reset all mutable state (cursor, chunk cache, sequential
/// read tracking). Each clone shares the underlying open file handles through
/// `File::try_clone` but has an independent seek position and read cache.
impl E01Reader {
    pub fn try_clone(&self) -> io::Result<Self> {
        let segment_files: io::Result<Vec<_>> =
            self.segment_files.iter().map(|f| f.try_clone()).collect();
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

    /// Re-open with fresh file handles, reusing the cached chunk table.
    /// Opens independent segment file descriptors (no shared file position)
    /// while sharing the parsed `Arc<chunk_table>` to avoid re-parsing headers.
    pub fn re_open(&self, source_path: &Path) -> io::Result<Self> {
        let mut segment_files: Vec<std::fs::File> = Vec::new();
        for seg_num in 1u32.. {
            let seg_path = build_segment_path(source_path, seg_num);
            match std::fs::File::open(&seg_path) {
                Ok(f) => segment_files.push(f),
                Err(e) if e.kind() == io::ErrorKind::NotFound => break,
                Err(e) => return Err(e),
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
}

impl E01Reader {
    fn read_chunk_uncached(&mut self, idx: u64) -> io::Result<Vec<u8>> {
        let (seg_idx, offset, compressed, stored_size) = chunk_entry(&self.chunk_table, idx)?;
        let chunk_bytes = self.chunk_size_sectors as usize * 512;

        if seg_idx >= self.segment_files.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "chunk references segment {} but only {} available",
                    seg_idx,
                    self.segment_files.len()
                ),
            ));
        }

        let file = &mut self.segment_files[seg_idx];
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
            let mut buf = vec![0u8; chunk_bytes];
            decoder
                .read_exact(&mut buf)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("zlib: {}", e)))?;
            Ok(buf)
        } else {
            let read_size = if stored_size == 0 {
                chunk_bytes
            } else {
                stored_size.min(chunk_bytes as u64) as usize
            };
            let mut buf = vec![0u8; chunk_bytes];
            if read_size == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "uncompressed chunk has zero stored size",
                ));
            }
            file.read_exact(&mut buf[..read_size])?;
            if read_size < chunk_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "failed to fill whole buffer",
                ));
            }
            Ok(buf)
        }
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
        if data.len() > SEQUENTIAL_CACHE_MAX_BYTES {
            return;
        }
        if self.chunk_cache.iter().any(|chunk| chunk.idx == idx) {
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
        let Some(start_idx) = idx.checked_add(1) else {
            return;
        };
        let end_idx = idx.saturating_add(SEQUENTIAL_PREFETCH_CHUNKS);
        for next_idx in start_idx..=end_idx {
            if next_idx as usize >= self.chunk_table.len() {
                break;
            }
            if self.cached_chunk(next_idx).is_some() {
                continue;
            }

            match self.read_chunk_uncached(next_idx) {
                Ok(data) => {
                    self.insert_cached_chunk(next_idx, Arc::<[u8]>::from(data.into_boxed_slice()))
                }
                Err(err) => {
                    tracing::debug!(
                        "E01 sequential prefetch stopped at chunk {}: {}",
                        next_idx,
                        err
                    );
                    break;
                }
            }
        }
    }

    fn read_bytes(&mut self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        if offset >= self.total_bytes {
            return Ok(0);
        }
        let mut total = 0usize;
        let mut off = offset;
        let csize = self.chunk_size_sectors as u64 * 512;
        while total < buf.len() && off < self.total_bytes {
            let chunk_idx = off / csize;
            let intra = (off % csize) as usize;
            let sequential = self
                .last_chunk_read
                .is_some_and(|last| chunk_idx == last || chunk_idx == last + 1);
            let data = self.read_chunk_cached(chunk_idx, sequential)?;
            let avail = (data.len() - intra).min(buf.len() - total);
            buf[total..total + avail].copy_from_slice(&data[intra..intra + avail]);
            total += avail;
            off += avail as u64;
            self.last_chunk_read = Some(chunk_idx);
        }
        Ok(total)
    }
}

impl Read for E01Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.read_bytes(buf, self.cursor)?;
        self.cursor += n as u64;
        Ok(n)
    }
}

impl Seek for E01Reader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.cursor = match pos {
            SeekFrom::Start(p) => p.min(self.total_bytes),
            SeekFrom::End(p) => ((self.total_bytes as i64) + p).max(0) as u64,
            SeekFrom::Current(p) => ((self.cursor as i64) + p).max(0) as u64,
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

fn build_chunk_table(
    sections: &[(String, u64, u64, Vec<u8>)],
    segment_sizes: &[u64],
    section_type: &str,
) -> Vec<(usize, u64, bool, u64)> {
    let mut chunk_table = Vec::new();

    for (stype, start_offset, next_offset, content) in sections {
        if stype != section_type || content.len() < V1_TABLE_HEADER_SIZE {
            continue;
        }

        let table_base = if content.len() >= 16 {
            u64::from_le_bytes(content[8..16].try_into().unwrap_or([0; 8]))
        } else {
            0
        };
        let number_of_entries =
            u32::from_le_bytes(content[0..4].try_into().unwrap_or([0; 4])) as usize;
        if number_of_entries == 0 {
            continue;
        }

        let segment = if segment_sizes.len() <= 1 {
            0
        } else {
            let mut cum = 0u64;
            let mut s = 0usize;
            for (i, &sz) in segment_sizes.iter().enumerate() {
                if table_base < cum + sz {
                    s = i;
                    break;
                }
                cum += sz;
                s = i;
            }
            s
        };

        let mut entries = Vec::new();
        for i in 0..number_of_entries {
            let off = V1_TABLE_HEADER_SIZE + i * 4;
            if off + 4 > content.len() {
                break;
            }
            let raw = u32::from_le_bytes(content[off..off + 4].try_into().unwrap_or([0; 4]));
            let compressed = raw & 0x8000_0000 != 0;
            let rel = (raw & 0x7FFF_FFFF) as u64;
            entries.push((rel, compressed));
        }

        for (i, (rel, compressed)) in entries.iter().copied().enumerate() {
            let abs_off = table_base + rel;
            let stored_size = if let Some((next_rel, _)) = entries.get(i + 1).copied() {
                next_rel.saturating_sub(rel)
            } else if abs_off < *start_offset {
                start_offset.saturating_sub(abs_off)
            } else if *next_offset > abs_off {
                next_offset.saturating_sub(abs_off)
            } else {
                0
            };

            chunk_table.push((segment, abs_off, compressed, stored_size));
        }
    }

    chunk_table
}

fn should_read_section_content(stype: &str) -> bool {
    stype == "volume" || stype.starts_with("disk") || stype == "table" || stype == "table2"
}

fn find_geometry(sections: &[(String, Vec<u8>)], file_len: u64) -> io::Result<(u64, u32)> {
    for (stype, content) in sections {
        if (stype == "volume" || stype.starts_with("disk")) && content.len() >= 24 {
            let sc = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
            let cks = chunk_sectors_from_geometry_section(stype, content);
            if sc > 0 && geometry_section_has_valid_sector_size(stype, content) {
                return Ok((sc, cks.max(1)));
            }
        }
    }
    for (_stype, content) in sections {
        if content.len() >= 24 {
            let sc = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
            if sc > 1_000_000 && sc < 100_000_000 && sc * 512 < file_len * 2 {
                return Ok((sc, 64));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "no geometry found",
    ))
}

fn chunk_sectors_from_geometry_section(stype: &str, content: &[u8]) -> u32 {
    let primary = if stype == "volume" && content.len() >= 16 {
        // Older EWF volume sections store sectors-per-chunk at offset 12.
        u32::from_le_bytes(content[12..16].try_into().unwrap_or([0; 4]))
    } else if content.len() >= 12 {
        // Disk sections store sectors-per-chunk at offset 8.
        u32::from_le_bytes(content[8..12].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    if primary > 0 {
        return primary;
    }
    if content.len() >= 12 {
        u32::from_le_bytes(content[8..12].try_into().unwrap_or([0; 4]))
    } else {
        64
    }
}

fn valid_bytes_per_sector(value: u32) -> bool {
    matches!(value, 0 | 512 | 1024 | 2048 | 4096)
}

fn geometry_section_has_valid_sector_size(stype: &str, content: &[u8]) -> bool {
    if stype == "volume" {
        return true;
    }
    if !stype.starts_with("disk") || content.len() < 16 {
        return true;
    }
    let bytes_per_sector = u32::from_le_bytes(content[12..16].try_into().unwrap_or([0; 4]));
    valid_bytes_per_sector(bytes_per_sector)
}

/// Build the path for segment N of an E01 image.
/// E.g., `image.E01` is segment 1 and `image.E02` is segment 2.
fn build_segment_path(first_segment: &Path, seg_num: u32) -> PathBuf {
    let ext = first_segment
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    let stem_with_ext = first_segment
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    // Preserve the image basename while advancing the numbered EWF extension.
    // Handle extensions like ".E01", ".e01", ".E01.001"
    let _base_ext = if ext.len() == 3 && ext.starts_with(['E', 'e']) {
        ext.to_uppercase()
    } else {
        ext.to_string()
    };

    if seg_num == 1 {
        return first_segment.to_path_buf();
    }

    // Determine the extension format: E01 -> EXX.
    let parent = first_segment.parent().unwrap_or_else(|| Path::new("."));
    let base_name = stem_with_ext.trim_end_matches(&ext.to_string());
    // Remove trailing dot
    let base_name = base_name.trim_end_matches('.');

    let new_ext = format!("E{:02}", seg_num);
    parent.join(format!("{}.{}", base_name, new_ext))
}

#[cfg(test)]
#[path = "../tests/unit/image_e01.rs"]
mod tests;
