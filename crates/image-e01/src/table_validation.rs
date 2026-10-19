//! Streaming validation of paged EWF offset arrays before publishing a plan.
use crate::{
    checksum::{adler32, page_digest, Adler32},
    geometry::{MAX_STORED_CHUNK_BYTES, V1_TABLE_HEADER_SIZE},
    paged_index::{TableSpan, PAGE_BYTES, PAGE_ENTRIES},
    sections::{cancelled, invalid, SectionSpan, SegmentSections},
};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    sync::atomic::AtomicBool,
};

pub(crate) fn validate_segment(
    file: &mut File,
    segment: &SegmentSections,
    first: u64,
    cancel: Option<&AtomicBool>,
) -> io::Result<Vec<TableSpan>> {
    if segment.sections.iter().any(|s| !s.checksum_valid) {
        return Err(invalid("E01 section descriptor checksum mismatch"));
    }
    match validate_series(file, segment, "table", first, cancel) {
        Ok(tables) if !tables.is_empty() => Ok(tables),
        result => {
            cancelled(cancel)?;
            match validate_series(file, segment, "table2", first, cancel) {
                Ok(tables) if !tables.is_empty() => Ok(tables),
                fallback => Err(result.err().or_else(|| fallback.err()).unwrap_or_else(|| {
                    invalid("E01 segment contains no usable table or table2 section")
                })),
            }
        }
    }
}

fn validate_series(
    file: &mut File,
    segment: &SegmentSections,
    kind: &str,
    first: u64,
    cancel: Option<&AtomicBool>,
) -> io::Result<Vec<TableSpan>> {
    let mut tables = Vec::new();
    let mut next = first;
    let mut pages = 0usize;
    for section in segment.sections.iter().filter(|s| s.kind == kind) {
        cancelled(cancel)?;
        let table = validate_table(file, segment, section, next, cancel)?;
        next = next
            .checked_add(table.count)
            .ok_or_else(|| invalid("E01 chunk count overflow"))?;
        pages = pages
            .checked_add(table.page_checksums.capacity())
            .ok_or_else(crate::open_limits::limit)?;
        if pages > crate::open_limits::MAX_METADATA_BYTES / 64 {
            return Err(crate::open_limits::limit());
        }
        tables.push(table);
    }
    Ok(tables)
}

fn validate_table(
    file: &mut File,
    segment: &SegmentSections,
    section: &SectionSpan,
    first: u64,
    cancel: Option<&AtomicBool>,
) -> io::Result<TableSpan> {
    let (start, size) = section.content_range(segment.file_len)?;
    file.seek(SeekFrom::Start(start))?;
    let mut header = [0; V1_TABLE_HEADER_SIZE];
    if size < 28 {
        return Err(invalid("E01 table section is too short"));
    }
    file.read_exact(&mut header)?;
    if adler32(&header[..20]) != u32::from_le_bytes(header[20..24].try_into().unwrap_or([0; 4])) {
        return Err(invalid("E01 table header checksum mismatch"));
    }
    let count = u64::from(u32::from_le_bytes(header[..4].try_into().unwrap_or([0; 4])));
    if count == 0 || 24 + count * 4 + 4 > size {
        return Err(invalid("E01 table entry count exceeds section length"));
    }
    let mut table = TableSpan {
        first_chunk: first,
        count,
        segment: section.segment,
        base: u64::from_le_bytes(header[8..16].try_into().unwrap_or([0; 8])),
        entries_offset: start + 24,
        last_end: 0,
        max_stored: 0,
        mirrored: section.kind == "table2",
        page_checksums: Vec::new(),
    };
    let mut checksum = Adler32::new();
    let mut buffer = [0; PAGE_BYTES];
    let mut previous = None;
    let mut remaining = count;
    while remaining > 0 {
        cancelled(cancel)?;
        let length = remaining.min(PAGE_ENTRIES) as usize * 4;
        file.read_exact(&mut buffer[..length])?;
        checksum.update(&buffer[..length]);
        table.page_checksums.push(page_digest(&buffer[..length]));
        for entry in buffer[..length].chunks_exact(4) {
            let relative =
                u64::from(u32::from_le_bytes(entry.try_into().unwrap_or([0; 4])) & 0x7fff_ffff);
            let absolute = table
                .base
                .checked_add(relative)
                .ok_or_else(|| invalid("E01 chunk offset overflow"))?;
            if let Some(before) = previous {
                validate_range(before, absolute, segment.file_len, &mut table)?;
            }
            previous = Some(absolute);
        }
        remaining -= length as u64 / 4;
    }
    let mut footer = [0; 4];
    file.read_exact(&mut footer)?;
    if checksum.finish() != u32::from_le_bytes(footer) {
        return Err(invalid("E01 offset array checksum mismatch"));
    }
    let last = previous.ok_or_else(|| invalid("E01 table has no offsets"))?;
    table.last_end = terminal_end(segment, section, last)?;
    validate_range(last, table.last_end, segment.file_len, &mut table)?;
    Ok(table)
}

fn validate_range(start: u64, end: u64, file_len: u64, table: &mut TableSpan) -> io::Result<()> {
    let length = end
        .checked_sub(start)
        .filter(|n| *n > 0 && *n <= MAX_STORED_CHUNK_BYTES)
        .ok_or_else(|| {
            invalid("E01 table offsets are not strictly increasing or stored range is invalid")
        })?;
    if end > file_len {
        return Err(invalid("E01 chunk extends beyond its segment"));
    }
    table.max_stored = table.max_stored.max(length);
    Ok(())
}

fn terminal_end(segment: &SegmentSections, section: &SectionSpan, last: u64) -> io::Result<u64> {
    for data in segment.sections.iter().filter(|s| s.kind == "sectors") {
        let (start, size) = data.content_range(segment.file_len)?;
        if last >= start && last < start + size {
            return Ok(start + size);
        }
    }
    // Legacy inline tables can precede the payload. A mirrored table following
    // a damaged primary must not include primary-table bytes in the last chunk.
    if let Some(table) = segment
        .sections
        .iter()
        .filter(|s| s.kind == "table" || s.kind == "table2")
        .filter(|s| s.start > last)
        .min_by_key(|s| s.start)
    {
        return Ok(table.start);
    }
    Ok(if section.next > last {
        section.next
    } else {
        segment.file_len
    })
}
