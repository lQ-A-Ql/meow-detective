use std::io;

pub(crate) const V1_TABLE_HEADER_SIZE: usize = 24;
const SUPPORTED_SECTOR_SIZES: [u32; 4] = [512, 1024, 2048, 4096];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct E01Geometry {
    pub(crate) sector_count: u64,
    pub(crate) sectors_per_chunk: u32,
    pub(crate) bytes_per_sector: u32,
}

impl E01Geometry {
    pub(crate) fn total_bytes(self) -> io::Result<u64> {
        self.sector_count
            .checked_mul(u64::from(self.bytes_per_sector))
            .ok_or_else(|| invalid_geometry("logical media size overflows u64"))
    }

    pub(crate) fn chunk_bytes(self) -> io::Result<u64> {
        u64::from(self.sectors_per_chunk)
            .checked_mul(u64::from(self.bytes_per_sector))
            .ok_or_else(|| invalid_geometry("chunk size overflows u64"))
    }
}

pub(crate) fn build_chunk_table(
    sections: &[(String, u64, u64, Vec<u8>)],
    segment_sizes: &[u64],
    section_type: &str,
) -> Vec<(usize, u64, bool, u64)> {
    let mut chunk_table = Vec::new();
    for (kind, start_offset, next_offset, content) in sections {
        if kind != section_type || content.len() < V1_TABLE_HEADER_SIZE {
            continue;
        }
        let table_base = u64::from_le_bytes(content[8..16].try_into().unwrap_or([0; 8]));
        let entry_count = u32::from_le_bytes(content[0..4].try_into().unwrap_or([0; 4])) as usize;
        if entry_count == 0 {
            continue;
        }
        let segment = segment_for_table(table_base, segment_sizes);
        let entries = parse_table_entries(content, entry_count);
        append_chunk_entries(
            &mut chunk_table,
            &entries,
            segment,
            table_base,
            *start_offset,
            *next_offset,
        );
    }
    chunk_table
}

fn segment_for_table(table_base: u64, segment_sizes: &[u64]) -> usize {
    if segment_sizes.len() <= 1 {
        return 0;
    }
    let mut cumulative = 0;
    let mut selected = 0;
    for (index, size) in segment_sizes.iter().enumerate() {
        selected = index;
        if table_base < cumulative + size {
            break;
        }
        cumulative += size;
    }
    selected
}

fn parse_table_entries(content: &[u8], entry_count: usize) -> Vec<(u64, bool)> {
    (0..entry_count)
        .filter_map(|index| {
            let offset = V1_TABLE_HEADER_SIZE + index * 4;
            let raw = content
                .get(offset..offset + 4)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_le_bytes)?;
            Some(((raw & 0x7FFF_FFFF) as u64, raw & 0x8000_0000 != 0))
        })
        .collect()
}

fn append_chunk_entries(
    table: &mut Vec<(usize, u64, bool, u64)>,
    entries: &[(u64, bool)],
    segment: usize,
    table_base: u64,
    start_offset: u64,
    next_offset: u64,
) {
    for (index, (relative, compressed)) in entries.iter().copied().enumerate() {
        let absolute = table_base + relative;
        let stored_size = entries
            .get(index + 1)
            .map(|(next, _)| next.saturating_sub(relative))
            .unwrap_or_else(|| {
                if absolute < start_offset {
                    start_offset.saturating_sub(absolute)
                } else {
                    next_offset.saturating_sub(absolute)
                }
            });
        table.push((segment, absolute, compressed, stored_size));
    }
}

pub(crate) fn should_read_section_content(kind: &str) -> bool {
    kind == "volume" || kind.starts_with("disk") || matches!(kind, "table" | "table2")
}

pub(crate) fn find_geometry(sections: &[(String, Vec<u8>)]) -> io::Result<E01Geometry> {
    let mut geometry_error = None;
    for (kind, content) in sections {
        if kind == "volume" || kind.starts_with("disk") {
            match parse_geometry_section(kind, content) {
                Ok(geometry) => return Ok(geometry),
                Err(error) => geometry_error = Some(error),
            }
        }
    }
    Err(geometry_error.unwrap_or_else(|| invalid_geometry("no geometry section found")))
}

fn parse_geometry_section(kind: &str, content: &[u8]) -> io::Result<E01Geometry> {
    if content.len() < 24 {
        return Err(invalid_geometry(format!(
            "{kind} section is too short: expected at least 24 bytes, got {}",
            content.len()
        )));
    }
    let sectors_per_chunk = u32::from_le_bytes(content[8..12].try_into().unwrap_or([0; 4]));
    let bytes_per_sector = u32::from_le_bytes(content[12..16].try_into().unwrap_or([0; 4]));
    let sector_count = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
    if sectors_per_chunk == 0 {
        return Err(invalid_geometry(format!(
            "{kind} section declares zero sectors per chunk"
        )));
    }
    if !SUPPORTED_SECTOR_SIZES.contains(&bytes_per_sector) {
        return Err(invalid_geometry(format!(
            "{kind} section declares unsupported sector size {bytes_per_sector}"
        )));
    }
    if sector_count == 0 {
        return Err(invalid_geometry(format!(
            "{kind} section declares zero media sectors"
        )));
    }
    let geometry = E01Geometry {
        sector_count,
        sectors_per_chunk,
        bytes_per_sector,
    };
    geometry.total_bytes()?;
    let chunk_bytes = geometry.chunk_bytes()?;
    usize::try_from(chunk_bytes)
        .map_err(|_| invalid_geometry("chunk size does not fit the current platform"))?;
    Ok(geometry)
}

fn invalid_geometry(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
