use std::io;

pub(crate) const V1_TABLE_HEADER_SIZE: usize = 24;
const V1_TABLE_CHECKSUM_SIZE: usize = 4;
const MAX_BYTES_PER_SECTOR: u32 = 64 * 1024;
pub(crate) const MAX_CHUNK_BYTES: u64 = 128 * 1024 * 1024;
pub(crate) const MAX_STORED_CHUNK_BYTES: u64 = MAX_CHUNK_BYTES + 1024 * 1024;

#[derive(Debug)]
pub(crate) struct E01Section {
    pub(crate) kind: String,
    pub(crate) segment_index: usize,
    pub(crate) start_offset: u64,
    pub(crate) next_offset: u64,
    pub(crate) content: Vec<u8>,
}

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
    sections: &[E01Section],
    segment_len: u64,
    section_type: &str,
) -> io::Result<Vec<(usize, u64, bool, u64)>> {
    let mut chunk_table = Vec::new();
    for section in sections
        .iter()
        .filter(|section| section.kind == section_type)
    {
        let entries = parse_table_section(section, segment_len)?;
        chunk_table.extend(entries);
    }
    Ok(chunk_table)
}

fn parse_table_section(
    section: &E01Section,
    segment_len: u64,
) -> io::Result<Vec<(usize, u64, bool, u64)>> {
    let content = &section.content;
    if content.len() < V1_TABLE_HEADER_SIZE + V1_TABLE_CHECKSUM_SIZE {
        return Err(invalid_table(section, "table section is too short"));
    }
    let entry_count = u32::from_le_bytes(content[0..4].try_into().unwrap_or([0; 4])) as usize;
    if entry_count == 0 {
        return Err(invalid_table(section, "table declares zero entries"));
    }
    let entries_bytes = entry_count
        .checked_mul(4)
        .and_then(|size| V1_TABLE_HEADER_SIZE.checked_add(size))
        .and_then(|size| size.checked_add(V1_TABLE_CHECKSUM_SIZE))
        .ok_or_else(|| invalid_table(section, "table entry length overflows usize"))?;
    if entries_bytes > content.len() {
        return Err(invalid_table(
            section,
            format!(
                "table declares {entry_count} entries but contains only {} bytes",
                content.len()
            ),
        ));
    }

    let table_base = u64::from_le_bytes(content[8..16].try_into().unwrap_or([0; 8]));
    let entries = parse_table_entries(section, content, entry_count)?;
    build_chunk_entries(section, segment_len, table_base, &entries)
}

fn parse_table_entries(
    section: &E01Section,
    content: &[u8],
    entry_count: usize,
) -> io::Result<Vec<(u64, bool)>> {
    let mut entries = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let offset = V1_TABLE_HEADER_SIZE + index * 4;
        let raw = content
            .get(offset..offset + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| invalid_table(section, "table entry is truncated"))?;
        let relative = u64::from(raw & 0x7FFF_FFFF);
        if entries
            .last()
            .is_some_and(|(previous, _)| relative <= *previous)
        {
            return Err(invalid_table(
                section,
                format!("table offsets are not strictly increasing at entry {index}"),
            ));
        }
        entries.push((relative, raw & 0x8000_0000 != 0));
    }
    Ok(entries)
}

fn build_chunk_entries(
    section: &E01Section,
    segment_len: u64,
    table_base: u64,
    entries: &[(u64, bool)],
) -> io::Result<Vec<(usize, u64, bool, u64)>> {
    let mut chunks = Vec::with_capacity(entries.len());
    for (index, (relative, compressed)) in entries.iter().copied().enumerate() {
        let absolute = table_base
            .checked_add(relative)
            .ok_or_else(|| invalid_table(section, "chunk offset overflows u64"))?;
        let end = chunk_end(section, segment_len, table_base, entries, index)?;
        let stored_size = end.checked_sub(absolute).ok_or_else(|| {
            invalid_table(section, format!("chunk {index} has a reversed byte range"))
        })?;
        validate_chunk_range(section, index, absolute, stored_size, segment_len)?;
        chunks.push((section.segment_index, absolute, compressed, stored_size));
    }
    Ok(chunks)
}

fn chunk_end(
    section: &E01Section,
    segment_len: u64,
    table_base: u64,
    entries: &[(u64, bool)],
    index: usize,
) -> io::Result<u64> {
    if let Some((next_relative, _)) = entries.get(index + 1) {
        return table_base
            .checked_add(*next_relative)
            .ok_or_else(|| invalid_table(section, "next chunk offset overflows u64"));
    }
    let absolute = table_base
        .checked_add(entries[index].0)
        .ok_or_else(|| invalid_table(section, "last chunk offset overflows u64"))?;
    if absolute < section.start_offset {
        Ok(section.start_offset)
    } else if section.next_offset > absolute {
        Ok(section.next_offset)
    } else {
        Ok(segment_len)
    }
}

fn validate_chunk_range(
    section: &E01Section,
    index: usize,
    offset: u64,
    stored_size: u64,
    segment_len: u64,
) -> io::Result<()> {
    let end = offset
        .checked_add(stored_size)
        .ok_or_else(|| invalid_table(section, "chunk byte range overflows u64"))?;
    if stored_size == 0 || stored_size > MAX_STORED_CHUNK_BYTES || end > segment_len {
        return Err(invalid_table(
            section,
            format!(
                "chunk {index} range is invalid: offset={offset} stored_length={stored_size} segment_length={segment_len}"
            ),
        ));
    }
    Ok(())
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
    if bytes_per_sector == 0 || bytes_per_sector > MAX_BYTES_PER_SECTOR {
        return Err(invalid_geometry(format!(
            "{kind} section declares invalid sector size {bytes_per_sector}"
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
    if chunk_bytes > MAX_CHUNK_BYTES {
        return Err(invalid_geometry(format!(
            "{kind} section declares chunk size {chunk_bytes} above the {MAX_CHUNK_BYTES} byte limit"
        )));
    }
    usize::try_from(chunk_bytes)
        .map_err(|_| invalid_geometry("chunk size does not fit the current platform"))?;
    Ok(geometry)
}

fn invalid_table(section: &E01Section, message: impl Into<String>) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "E01 segment {} {} section at offset {}: {}",
            section.segment_index,
            section.kind,
            section.start_offset,
            message.into()
        ),
    )
}

fn invalid_geometry(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
