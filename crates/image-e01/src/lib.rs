use evidence_core::{EvidenceReader, ReaderInfo};
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// E01 (EWF) image reader. Section-based parser (reads metadata from first 2MB).
/// Supports uncompressed chunks with cross-chunk reads. Compressed = Unsupported.
pub struct E01Reader {
    info: ReaderInfo,
    total_bytes: u64,
    pub sectors_per_chunk: u64,
    chunk_table: HashMap<u64, ChunkEntry>,
    file: std::fs::File,
    cursor: u64,
}

struct ChunkEntry {
    file_offset: u64,
    compressed: bool,
    data_size: u32,
}

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let file_len = file.seek(SeekFrom::End(0))? as usize;
        file.seek(SeekFrom::Start(0))?;
        let read_limit = 2_097_152usize.min(file_len);
        let mut data = vec![0u8; read_limit];
        file.read_exact(&mut data)?;

        if data.len() < 3 || &data[0..3] != b"EVF" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not EWF"));
        }

        // Try ASCII header first (tab-separated fields ending with LF)
        let lf_pos = data[3..].iter().position(|&b| b == 0x0A);
        let hdr_end = lf_pos.map(|p| (3 + p).min(data.len())).unwrap_or(data.len());
        let hdr = String::from_utf8_lossy(&data[3..hdr_end]);
        let fields: Vec<u64> = hdr.split('\t').filter_map(|p| p.parse::<u64>().ok()).collect();

        let (sectors_count, chunk_size_raw) = if fields.len() >= 2 {
            (fields[fields.len() - 1], fields[0] as u32)
        } else {
            // Binary header: find "header" section, read version, use known offsets
            match find_sectors_from_header(&data) {
                Some((sc, cs)) => (sc, cs),
                None => return Err(io::Error::new(io::ErrorKind::InvalidData, "cannot determine sector count")),
            }
        };

        if sectors_count == 0 { return Err(io::Error::new(io::ErrorKind::InvalidData, "zero sectors")); }
        let total_bytes = sectors_count * 512;
        let spc = if chunk_size_raw > 0 { chunk_size_raw as u64 / 512 } else { 64 };

        // Parse "table" sections
        let mut chunk_table = HashMap::new();
        let mut chunk_base = 0u64;
        let mut sp = 0usize;
        while let Some(tpos) = data[sp..].windows(6).position(|w| w == b"table\x00") {
            let abs = sp + tpos + 6;
            if abs + 12 > data.len() { break; }
            let n = u32::from_le_bytes(data[abs+8..abs+12].try_into().unwrap()) as usize;
            if n == 0 || abs + 12 + n * 12 > data.len() { sp = abs + 12; continue; }
            let mut offs = Vec::with_capacity(n);
            for i in 0..n {
                let o = abs + 12 + i * 12;
                let fo = u64::from_le_bytes(data[o..o+8].try_into().unwrap());
                let comp = u32::from_le_bytes(data[o+8..o+12].try_into().unwrap()) != 0;
                offs.push((fo, comp));
            }
            let sb = abs + 12 + n * 12;
            if sb + n * 8 > data.len() { sp = sb; continue; }
            for (i, (fo, comp)) in offs.iter().enumerate() {
                let so = sb + i * 4;
                let sz = u32::from_le_bytes(data[so..so+4].try_into().unwrap());
                chunk_table.insert(chunk_base + i as u64, ChunkEntry { file_offset: *fo, compressed: *comp, data_size: sz });
            }
            chunk_base += n as u64;
            sp = sb + n * 4 + n * 4;
        }

        Ok(Self {
            info: ReaderInfo { path: path.to_path_buf(), size: total_bytes, kind: "e01".into() },
            total_bytes, sectors_per_chunk: spc, chunk_table, file, cursor: 0,
        })
    }

    fn read_chunk(&mut self, idx: u64) -> io::Result<Vec<u8>> {
        let e = self.chunk_table.get(&idx)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk not found"))?;
        if e.compressed { return Err(io::Error::new(io::ErrorKind::Unsupported, "compressed chunk")); }
        self.file.seek(SeekFrom::Start(e.file_offset))?;
        let mut buf = vec![0u8; e.data_size as usize];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_bytes(&mut self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        if offset >= self.total_bytes { return Ok(0); }
        let mut total = 0usize;
        let mut off = offset;
        while total < buf.len() && off < self.total_bytes {
            let csize = self.sectors_per_chunk * 512;
            let chunk_idx = off / csize;
            let intra = (off % csize) as usize;
            let data = self.read_chunk(chunk_idx)?;
            if intra >= data.len() { break; }
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
        }.min(self.total_bytes);
        Ok(self.cursor)
    }
}

impl EvidenceReader for E01Reader {
    fn info(&self) -> &ReaderInfo { &self.info }
}

fn find_sectors_from_header(data: &[u8]) -> Option<(u64, u32)> {
    let hpos = data.windows(7).position(|w| w == b"header\x00")?;
    let sec = &data[hpos + 7..];
    if sec.len() < 256 { return None; }
    let version = u32::from_le_bytes(sec[0..4].try_into().ok()?);
    let (sc_off, cs_off) = match version {
        1 => (24, 76),
        _ => (248, 80),
    };
    if sc_off + 8 > sec.len() || cs_off + 4 > sec.len() { return None; }
    let sc = u64::from_le_bytes(sec[sc_off..sc_off+8].try_into().ok()?);
    let cs = u32::from_le_bytes(sec[cs_off..cs_off+4].try_into().ok()?);
    if sc == 0 { None } else { Some((sc, cs)) }
}
