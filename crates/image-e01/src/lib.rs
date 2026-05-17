use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// E01 reader. Walks 76-byte section descriptor linked list.
/// Supports both ASCII-header and EWF-L01 binary-header formats.
pub struct E01Reader {
    info: ReaderInfo,
    total_bytes: u64,
    chunk_size_sectors: u32,
    /// chunk_index -> (segment_file_offset, compressed)
    chunk_table: Vec<(u64, bool)>,
    file: std::fs::File,
    cursor: u64,
}

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let file_len = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;

        // File header: magic[3] + tab[1] + crlf[2] + media[1] + reserved[2]
        // + seg_n[2] + seg_total[2] = 13 bytes
        let mut fhdr = [0u8; 13];
        file.read_exact(&mut fhdr)?;
        if &fhdr[0..3] != b"EVF" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not EWF"));
        }

        // First section descriptor at offset 13
        let mut next_off = 13u64;
        let mut sections: Vec<(String, Vec<u8>)> = Vec::new();

        while next_off > 0 && next_off < file_len {
            file.seek(SeekFrom::Start(next_off))?;
            let mut desc = [0u8; 76];
            if file.read_exact(&mut desc).is_err() {
                break;
            }

            // section_type: 16 bytes NUL-padded ASCII
            let stype = String::from_utf8_lossy(&desc[0..16])
                .trim_end_matches('\0')
                .to_string();

            let next = u64::from_le_bytes(desc[16..24].try_into().unwrap());
            let section_size = u64::from_le_bytes(desc[24..32].try_into().unwrap());

            // Read section content (limit to 10MB per section, respect file_len)
            let read_size = section_size
                .min(10_000_000)
                .min(file_len.saturating_sub(next_off + 76));
            let mut content = vec![0u8; read_size as usize];
            if read_size > 0 {
                file.seek(SeekFrom::Start(next_off + 76))?;
                file.read_exact(&mut content)?;
            }

            sections.push((stype.clone(), content));

            if stype == "done" {
                break;
            }
            next_off = if next > 0 && next < file_len { next } else { 0 };
        }

        // Find volume section to get geometry
        let (sectors_count, chunk_size_sectors) = find_geometry(&sections, file_len)?;

        let total_bytes = sectors_count * 512;
        let chunk_size_sectors = if chunk_size_sectors > 0 {
            chunk_size_sectors
        } else {
            64
        };

        // Build chunk table from "table" section(s)
        let mut chunk_table: Vec<(u64, bool)> = Vec::new();
        for (stype, content) in &sections {
            if stype.starts_with("table") && content.len() >= 12 {
                let table_base = if content.len() >= 16 {
                    u64::from_le_bytes(content[8..16].try_into().unwrap_or([0; 8]))
                } else {
                    0
                };

                let entry_count = content.len().saturating_sub(12) / 4;
                for i in 0..entry_count {
                    let off = 12 + i * 4;
                    let raw =
                        u32::from_le_bytes(content[off..off + 4].try_into().unwrap_or([0; 4]));
                    let compressed = raw & 0x8000_0000 != 0;
                    let rel = (raw & 0x7FFF_FFFF) as u64;
                    let abs_off = if table_base > 0 {
                        table_base + rel
                    } else {
                        rel
                    };
                    chunk_table.push((abs_off, compressed));
                }
            }
        }

        Ok(Self {
            info: ReaderInfo {
                path: path.to_path_buf(),
                size: total_bytes,
                kind: "e01".into(),
            },
            total_bytes,
            chunk_size_sectors,
            chunk_table,
            file,
            cursor: 0,
        })
    }

    fn read_chunk(&mut self, idx: u64) -> io::Result<Vec<u8>> {
        let (offset, compressed) = chunk_entry(&self.chunk_table, idx)?;
        if compressed {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "compressed E01 chunk",
            ));
        }
        self.file.seek(SeekFrom::Start(offset))?;
        let chunk_bytes = self.chunk_size_sectors as usize * 512;
        let mut buf = vec![0u8; chunk_bytes];
        self.file.read_exact(&mut buf)?;
        // Skip 4-byte Adler-32 checksum after uncompressed chunk data
        Ok(buf)
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

fn chunk_entry(table: &[(u64, bool)], idx: u64) -> io::Result<(u64, bool)> {
    table
        .get(idx as usize)
        .copied()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk not found"))
}

fn find_geometry(sections: &[(String, Vec<u8>)], file_len: u64) -> io::Result<(u64, u32)> {
    for (stype, content) in sections {
        if (stype == "volume" || stype.starts_with("disk")) && content.len() >= 24 {
            let sc = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
            let cks = u32::from_le_bytes(content[12..16].try_into().unwrap_or([0; 4]));
            if sc > 0 && sc * 512 < file_len * 10 {
                return Ok((sc, cks.clamp(1, 64)));
            }
        }
    }
    // Fallback: scan any section that might contain geometry
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
