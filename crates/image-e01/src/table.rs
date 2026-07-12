use std::io;

pub(crate) const V1_TABLE_HEADER_SIZE: usize = 24;

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

pub(crate) fn find_geometry(
    sections: &[(String, Vec<u8>)],
    file_len: u64,
) -> io::Result<(u64, u32)> {
    for (kind, content) in sections {
        if (kind == "volume" || kind.starts_with("disk")) && content.len() >= 24 {
            let sectors = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
            if sectors > 0 && geometry_section_has_valid_sector_size(kind, content) {
                return Ok((
                    sectors,
                    chunk_sectors_from_geometry_section(kind, content).max(1),
                ));
            }
        }
    }
    for (_, content) in sections {
        if content.len() < 24 {
            continue;
        }
        let sectors = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
        if sectors > 1_000_000 && sectors < 100_000_000 && sectors * 512 < file_len * 2 {
            return Ok((sectors, 64));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "no geometry found",
    ))
}

fn chunk_sectors_from_geometry_section(kind: &str, content: &[u8]) -> u32 {
    let primary = if kind == "volume" && content.len() >= 16 {
        u32::from_le_bytes(content[12..16].try_into().unwrap_or([0; 4]))
    } else if content.len() >= 12 {
        u32::from_le_bytes(content[8..12].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    if primary > 0 {
        primary
    } else if content.len() >= 12 {
        u32::from_le_bytes(content[8..12].try_into().unwrap_or([0; 4]))
    } else {
        64
    }
}

fn geometry_section_has_valid_sector_size(kind: &str, content: &[u8]) -> bool {
    if kind == "volume" || !kind.starts_with("disk") || content.len() < 16 {
        return true;
    }
    matches!(
        u32::from_le_bytes(content[12..16].try_into().unwrap_or([0; 4])),
        0 | 512 | 1024 | 2048 | 4096
    )
}
