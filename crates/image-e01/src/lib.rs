use evidence_core::{EvidenceReader, ReaderInfo};
use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    chunk_table: Vec<(usize, u64, bool, u64)>, // (segment, offset, compressed, stored_size)
    segment_files: Vec<std::fs::File>,
    cursor: u64,
    chunk_cache: VecDeque<CachedChunk>,
    chunk_cache_bytes: usize,
    last_chunk_read: Option<u64>,
}

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let base = path.with_extension("");
        let _stem = base.file_stem().unwrap_or_default().to_string_lossy();

        let mut segment_files: Vec<std::fs::File> = Vec::new();
        // Open .E01, .E02, ... until file not found
        for seg_num in 1u32.. {
            let seg_path = build_segment_path(path, seg_num);
            match std::fs::File::open(&seg_path) {
                Ok(f) => segment_files.push(f),
                Err(e) if e.kind() == io::ErrorKind::NotFound => break,
                Err(e) => return Err(e),
            }
        }

        let mut file = &segment_files[0];
        let file_len = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;

        // File header: 13 bytes
        let mut fhdr = [0u8; 13];
        file.read_exact(&mut fhdr)?;
        if &fhdr[0..3] != b"EVF" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not EWF"));
        }

        // Walk section descriptor linked list
        // Track visited offsets to detect cycles in malformed E01 files
        let mut visited_offsets = std::collections::HashSet::<u64>::new();
        let mut next_off = 13u64;
        let mut sections: Vec<(String, u64, u64, Vec<u8>)> = Vec::new();

        while next_off > 0 && next_off < file_len {
            if !visited_offsets.insert(next_off) {
                tracing::warn!(
                    "E01: cycle detected in section chain at offset 0x{:X}, stopping",
                    next_off
                );
                break;
            }
            file.seek(SeekFrom::Start(next_off))?;
            let mut desc = [0u8; 76];
            if file.read_exact(&mut desc).is_err() {
                break;
            }

            let stype = String::from_utf8_lossy(&desc[0..16])
                .trim_end_matches('\0')
                .to_string();

            let next = u64::from_le_bytes(desc[16..24].try_into().unwrap_or([0; 8]));
            let section_size = u64::from_le_bytes(desc[24..32].try_into().unwrap_or([0; 8]));

            let data_start = next_off.saturating_add(SECTION_DESCRIPTOR_SIZE);
            let size_from_section = section_size.saturating_sub(SECTION_DESCRIPTOR_SIZE);
            let size_from_next = if next > data_start && next <= file_len {
                next - data_start
            } else {
                0
            };
            let read_size = if stype == "done" {
                0
            } else if size_from_section > 0 && size_from_next > 0 {
                size_from_section.min(size_from_next)
            } else {
                size_from_section.max(size_from_next)
            }
            .min(10_000_000)
            .min(file_len.saturating_sub(data_start));
            let mut content = vec![0u8; read_size as usize];
            if read_size > 0 {
                file.seek(SeekFrom::Start(data_start))?;
                file.read_exact(&mut content)?;
            }

            sections.push((stype.clone(), next_off, next, content));

            if stype == "done" {
                break;
            }
            next_off = if next > 0 && next < file_len { next } else { 0 };
        }

        let section_views: Vec<(String, Vec<u8>)> = sections
            .iter()
            .map(|(stype, _start, _next, content)| (stype.clone(), content.clone()))
            .collect();
        let (sectors_count, chunk_size_sectors) = find_geometry(&section_views, file_len)?;
        let total_bytes = sectors_count * 512;
        let cks = if chunk_size_sectors > 0 {
            chunk_size_sectors
        } else {
            64
        };
        let chunk_bytes = cks as u64 * 512;
        let expected_chunks = if chunk_bytes > 0 {
            total_bytes.div_ceil(chunk_bytes)
        } else {
            0
        };

        // Pre-fetch segment sizes
        let segment_sizes: Vec<u64> = segment_files
            .iter()
            .map(|f| f.metadata().map(|m| m.len()).unwrap_or(0))
            .collect();

        // Prefer the main `table` sections. `table2` carries the same chunk group metadata
        // and should only be used as a fallback when the primary table is unusable.
        let mut chunk_table = build_chunk_table(&sections, &segment_sizes, "table");
        if expected_chunks > 0 && chunk_table.len() as u64 != expected_chunks {
            let fallback = build_chunk_table(&sections, &segment_sizes, "table2");
            if fallback.len() as u64 == expected_chunks {
                chunk_table = fallback;
            }
        }
        if chunk_table.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no usable chunk table found",
            ));
        }

        Ok(Self {
            info: ReaderInfo {
                path: path.to_path_buf(),
                size: total_bytes,
                kind: "e01".into(),
            },
            total_bytes,
            chunk_size_sectors: cks,
            chunk_table,
            segment_files,
            cursor: 0,
            chunk_cache: VecDeque::new(),
            chunk_cache_bytes: 0,
            last_chunk_read: None,
        })
    }

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

    #[cfg(test)]
    fn cached_chunk_indices_for_test(&self) -> Vec<u64> {
        self.chunk_cache.iter().map(|chunk| chunk.idx).collect()
    }

    #[cfg(test)]
    fn cache_bytes_for_test(&self) -> usize {
        self.chunk_cache_bytes
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

fn find_geometry(sections: &[(String, Vec<u8>)], file_len: u64) -> io::Result<(u64, u32)> {
    for (stype, content) in sections {
        if (stype == "volume" || stype.starts_with("disk")) && content.len() >= 24 {
            let sc = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
            let cks = u32::from_le_bytes(content[8..12].try_into().unwrap_or([0; 4]));
            if sc > 0 && sc * 512 < file_len * 10 {
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

/// Build the path for segment N of an E01 image.
/// E.g., "image.E01" → segment 1, "image.E02" → segment 2, etc.
fn build_segment_path(first_segment: &Path, seg_num: u32) -> PathBuf {
    let ext = first_segment
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    let stem_with_ext = first_segment
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    // E01 → E02, e01 → e02, E01 → E02 etc.
    // Handle extensions like ".E01", ".e01", ".E01.001"
    let _base_ext = if ext.len() == 3 && ext.starts_with(['E', 'e']) {
        ext.to_uppercase()
    } else {
        ext.to_string()
    };

    if seg_num == 1 {
        return first_segment.to_path_buf();
    }

    // Determine the extension format: E01 → EXX
    let parent = first_segment.parent().unwrap_or_else(|| Path::new("."));
    let base_name = stem_with_ext.trim_end_matches(&ext.to_string());
    // Remove trailing dot
    let base_name = base_name.trim_end_matches('.');

    let new_ext = format!("E{:02}", seg_num);
    parent.join(format!("{}.{}", base_name, new_ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_build_segment_path_first() {
        let path = Path::new("/data/image.E01");
        let seg = build_segment_path(path, 1);
        assert_eq!(seg, Path::new("/data/image.E01"));
    }

    #[test]
    fn test_build_segment_path_second() {
        let path = Path::new("/data/image.E01");
        let seg = build_segment_path(path, 2);
        assert_eq!(seg, Path::new("/data/image.E02"));
    }

    #[test]
    fn test_build_segment_path_third() {
        let path = Path::new("/data/image.E01");
        let seg = build_segment_path(path, 3);
        assert_eq!(seg, Path::new("/data/image.E03"));
    }

    #[test]
    fn test_build_segment_path_lowercase() {
        let path = Path::new("/data/image.e01");
        let seg = build_segment_path(path, 2);
        assert_eq!(seg, Path::new("/data/image.E02"));
    }

    #[test]
    fn test_section_descriptor_size() {
        assert_eq!(SECTION_DESCRIPTOR_SIZE, 76);
    }

    #[test]
    fn test_v1_table_header_size() {
        assert_eq!(V1_TABLE_HEADER_SIZE, 24);
    }

    #[test]
    fn sequential_reads_populate_bounded_neighbor_cache() {
        let dir = std::env::temp_dir().join("e01_cache_prefetch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.E01");
        write_multichunk_e01(&path, 6).unwrap();

        let mut reader = E01Reader::open(&path).unwrap();
        let chunk_bytes = reader.chunk_size_sectors as usize * 512;
        let mut buf = vec![0u8; chunk_bytes + 1];
        reader.read_exact(&mut buf).unwrap();

        let cached = reader.cached_chunk_indices_for_test();
        assert!(cached.contains(&0));
        assert!(cached.contains(&1));
        assert!(cached.contains(&2));
        assert!(cached.contains(&3));
        assert!(reader.cache_bytes_for_test() <= SEQUENTIAL_CACHE_MAX_BYTES);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seek_resets_sequential_prefetch_hint() {
        let dir = std::env::temp_dir().join("e01_cache_seek_reset");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.E01");
        write_multichunk_e01(&path, 6).unwrap();

        let mut reader = E01Reader::open(&path).unwrap();
        let chunk_bytes = reader.chunk_size_sectors as u64 * 512;
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).unwrap();
        reader.seek(SeekFrom::Start(chunk_bytes * 4)).unwrap();
        reader.read_exact(&mut byte).unwrap();

        let cached = reader.cached_chunk_indices_for_test();
        assert!(cached.contains(&0));
        assert!(cached.contains(&4));
        assert!(!cached.contains(&5));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn write_multichunk_e01(path: &Path, chunk_count: u32) -> io::Result<()> {
        let chunk_sectors: u32 = 8;
        let sectors = chunk_count as u64 * chunk_sectors as u64;
        let chunk_bytes = (chunk_sectors * 512) as usize;

        let mut f = std::fs::File::create(path)?;
        f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")?;

        let mut vol = vec![0u8; 36];
        vol[12..16].copy_from_slice(&chunk_sectors.to_le_bytes());
        vol[16..24].copy_from_slice(&sectors.to_le_bytes());

        let volume_desc_offset = 13u64;
        let table_desc_offset = volume_desc_offset + SECTION_DESCRIPTOR_SIZE + vol.len() as u64;
        let table_len = V1_TABLE_HEADER_SIZE + chunk_count as usize * 4 + 4;
        let done_desc_offset = table_desc_offset + SECTION_DESCRIPTOR_SIZE + table_len as u64;
        let chunk0_offset = done_desc_offset + SECTION_DESCRIPTOR_SIZE;

        f.write_all(&test_section_desc(
            "volume",
            table_desc_offset,
            SECTION_DESCRIPTOR_SIZE + vol.len() as u64,
        ))?;
        f.write_all(&vol)?;

        let mut table = vec![0u8; table_len];
        table[0..4].copy_from_slice(&chunk_count.to_le_bytes());
        table[8..16].copy_from_slice(&chunk0_offset.to_le_bytes());
        for idx in 0..chunk_count as usize {
            let rel = (idx * chunk_bytes) as u32;
            let pos = V1_TABLE_HEADER_SIZE + idx * 4;
            table[pos..pos + 4].copy_from_slice(&rel.to_le_bytes());
        }
        f.write_all(&test_section_desc(
            "table",
            done_desc_offset,
            SECTION_DESCRIPTOR_SIZE + table.len() as u64,
        ))?;
        f.write_all(&table)?;

        f.write_all(&test_section_desc("done", 0, 0))?;

        for idx in 0..chunk_count {
            let mut chunk = vec![idx as u8; chunk_bytes];
            chunk[0..4].copy_from_slice(&idx.to_le_bytes());
            f.write_all(&chunk)?;
        }
        f.flush()
    }

    fn test_section_desc(stype: &str, next: u64, size: u64) -> [u8; 76] {
        let mut desc = [0u8; 76];
        let bytes = stype.as_bytes();
        desc[0..bytes.len().min(16)].copy_from_slice(&bytes[..bytes.len().min(16)]);
        desc[16..24].copy_from_slice(&next.to_le_bytes());
        desc[24..32].copy_from_slice(&size.to_le_bytes());
        desc
    }
}
