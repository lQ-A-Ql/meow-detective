use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const SECTION_DESCRIPTOR_SIZE: u64 = 76;
const V1_TABLE_HEADER_SIZE: usize = 24;

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
        let mut next_off = 13u64;
        let mut sections: Vec<(String, u64, u64, Vec<u8>)> = Vec::new();

        while next_off > 0 && next_off < file_len {
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
        })
    }

    fn read_chunk(&mut self, idx: u64) -> io::Result<Vec<u8>> {
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
            let data = self.read_chunk(chunk_idx)?;
            let avail = (data.len() - intra).min(buf.len() - total);
            buf[total..total + avail].copy_from_slice(&data[intra..intra + avail]);
            total += avail;
            off += avail as u64;
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
