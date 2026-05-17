use evidence_core::{EvidenceReader, ReaderInfo};
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// E01 (EWF) image reader.
/// Parses header fields, section table, chunk table. Reads uncompressed data chunks.
/// Compressed chunks return an Unsupported error.
pub struct E01Reader {
    info: ReaderInfo,
    total_sectors: u64,
    chunk_size: u32,
    /// chunk_index -> (file_offset, compressed, chunk_data_size)
    chunk_table: HashMap<u64, ChunkEntry>,
    file: std::fs::File,
}

struct ChunkEntry {
    offset: u64,
    compressed: bool,
    data_size: u32,
}

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = std::fs::File::open(path)?;

        // read header magic
        let mut magic = [0u8; 3];
        file.read_exact(&mut magic)?;
        if &magic != b"EVF" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not an EWF file"));
        }
        // skip remaining magic bytes + LF
        file.seek(SeekFrom::Current(13 + 1))?;

        // header fields: tab-separated numbers until newline
        let mut hdr_line = Vec::new();
        loop {
            let mut b = [0u8; 1];
            file.read_exact(&mut b)?;
            if b[0] == 0x0A { break; }
            hdr_line.push(b[0]);
        }
        let s = String::from_utf8_lossy(&hdr_line);
        let parts = s.split('\t').filter_map(|p| p.parse::<u64>().ok());
        let _fields: Vec<u64> = parts.collect();

        // skip strings: case_number(0) desc(0) examiner(0) notes(0) x2
        skip_null_str(&mut file)?;
        skip_null_str(&mut file)?;
        skip_null_str(&mut file)?;
        skip_null_str(&mut file)?;

        let sectors_count = read_u64(&mut file)?;
        let chunk_size_raw = read_u32(&mut file)?;
        let _error_gran = read_u32(&mut file)?;

        // section list starts at (sectors + 1) * 512
        let body = (sectors_count + 1) * 512;
        file.seek(SeekFrom::Start(body))?;
        let first_section = read_u64(&mut file)?;
        file.seek(SeekFrom::Start(first_section))?;

        // volume section — just skip it
        skip_section(&mut file, "volume")?;

        let mut chunk_table = HashMap::new();
        let mut chunk_base = 0u64;

        loop {
            let pos = file.stream_position()?;
            let end = file.seek(SeekFrom::End(0))?;
            file.seek(SeekFrom::Start(pos))?;
            if pos >= end { break; }

            let stype = match read_null_str(&mut file) {
                Ok(s) => s,
                Err(_) => break,
            };
            if stype != "table" { break; }

            let _next = read_u64(&mut file)?;
            let n_chunks = read_u32(&mut file)?;
            let _pad = read_u32(&mut file)?;

            let mut offsets = Vec::with_capacity(n_chunks as usize);
            for _ in 0..n_chunks {
                let off = read_u64(&mut file)?;
                let comp = read_u32(&mut file)? != 0;
                offsets.push((off, comp));
            }

            let mut sizes = Vec::with_capacity(n_chunks as usize);
            for _ in 0..n_chunks {
                sizes.push(read_u32(&mut file)?);
            }

            for (i, sz) in sizes.iter().enumerate() {
                let (off, comp) = offsets[i];
                chunk_table.insert(chunk_base + i as u64, ChunkEntry { offset: off, compressed: comp, data_size: *sz });
            }
            // skip CRC table
            file.seek(SeekFrom::Current(n_chunks as i64 * 4))?;
            chunk_base += n_chunks as u64;
        }

        Ok(Self {
            info: ReaderInfo { path: path.to_path_buf(), size: sectors_count * 512, kind: "e01".into() },
            total_sectors: sectors_count,
            chunk_size: chunk_size_raw,
            chunk_table,
            file,
        })
    }

    fn read_chunk_data(&mut self, idx: u64) -> io::Result<Vec<u8>> {
        let entry = self.chunk_table.get(&idx)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk not found"))?;
        if entry.compressed {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "compressed E01 chunk"));
        }
        self.file.seek(SeekFrom::Start(entry.offset))?;
        let mut buf = vec![0u8; entry.data_size as usize];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_at_sector(&mut self, buf: &mut [u8], sector: u64) -> io::Result<usize> {
        let sectors_per_chunk = if self.chunk_size > 0 { self.chunk_size as u64 / 512 } else { 1 };
        let chunk_idx = sector / sectors_per_chunk;
        let offset = (sector % sectors_per_chunk) * 512;
        let data = self.read_chunk_data(chunk_idx)?;
        let start = offset as usize;
        let end = (start + buf.len()).min(data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&data[start..end]);
        Ok(n)
    }
}

impl Read for E01Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pos = self.file.stream_position()?;
        self.read_at_sector(buf, pos / 512)
    }
}

impl Seek for E01Reader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::End(p) => ((self.total_sectors * 512) as i64 + p).max(0) as u64,
            SeekFrom::Current(p) => {
                let cur = self.file.stream_position()?;
                ((cur as i64) + p).max(0) as u64
            }
        };
        self.file.seek(SeekFrom::Start(new))
    }
}

impl EvidenceReader for E01Reader {
    fn info(&self) -> &ReaderInfo { &self.info }
}

fn read_u64(f: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8]; f.read_exact(&mut b)?; Ok(u64::from_le_bytes(b))
}
fn read_u32(f: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4]; f.read_exact(&mut b)?; Ok(u32::from_le_bytes(b))
}
fn skip_null_str(f: &mut impl Read) -> io::Result<()> {
    loop { let mut b = [0u8; 1]; f.read_exact(&mut b)?; if b[0] == 0 { return Ok(()); } }
}
fn read_null_str(f: &mut impl Read) -> io::Result<String> {
    let mut buf = Vec::new();
    loop { let mut b = [0u8; 1]; f.read_exact(&mut b)?; if b[0] == 0 { break; } buf.push(b[0]); }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}
fn skip_section(f: &mut std::fs::File, _expected: &str) -> io::Result<()> {
    let _sname = read_null_str(f)?;
    let _pad = read_u64(f)?;
    Ok(())
}
