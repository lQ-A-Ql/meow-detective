use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use evidence_core::ReaderInfo;

use super::{build_segment_path, E01Reader, SECTION_DESCRIPTOR_SIZE};
use crate::table::{
    build_chunk_table, find_geometry, should_read_section_content, E01Geometry, E01Section,
};

const MAX_SECTION_CONTENT_BYTES: u64 = 64 * 1024 * 1024;

struct SegmentSections {
    file_len: u64,
    sections: Vec<E01Section>,
}

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut segment_files = open_segment_files(path)?;
        let segment_sections = read_all_segment_sections(&mut segment_files)?;
        let geometry = geometry(&segment_sections)?;
        let total_bytes = geometry.total_bytes()?;
        let chunk_table =
            select_chunk_table(&segment_sections, total_bytes, geometry.chunk_bytes()?)?;
        Ok(Self::from_parts(
            ReaderInfo {
                path: path.to_path_buf(),
                size: total_bytes,
                kind: "e01".into(),
            },
            total_bytes,
            geometry.sectors_per_chunk,
            geometry.bytes_per_sector,
            chunk_table,
            segment_files,
        ))
    }
}

fn open_segment_files(path: &Path) -> io::Result<Vec<std::fs::File>> {
    let mut files = Vec::new();
    for segment in 1u32.. {
        match std::fs::File::open(build_segment_path(path, segment)) {
            Ok(file) => files.push(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    if files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no E01 segments found",
        ));
    }
    Ok(files)
}

fn read_all_segment_sections(
    segment_files: &mut [std::fs::File],
) -> io::Result<Vec<SegmentSections>> {
    segment_files
        .iter_mut()
        .enumerate()
        .map(|(segment_index, file)| {
            let file_len = verify_header(file, segment_index)?;
            let sections = read_sections(file, file_len, segment_index)?;
            Ok(SegmentSections { file_len, sections })
        })
        .collect()
}

fn verify_header(file: &mut std::fs::File, segment_index: usize) -> io::Result<u64> {
    let file_len = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; 13];
    file.read_exact(&mut header).map_err(|error| {
        invalid_data(format!(
            "E01 segment {segment_index} header is truncated: {error}"
        ))
    })?;
    if &header[0..3] != b"EVF" {
        return Err(invalid_data(format!(
            "E01 segment {segment_index} has an invalid EWF signature"
        )));
    }
    Ok(file_len)
}

fn read_sections(
    file: &mut std::fs::File,
    file_len: u64,
    segment_index: usize,
) -> io::Result<Vec<E01Section>> {
    let mut visited = HashSet::new();
    let mut next_offset = Some(13u64);
    let mut sections = Vec::new();
    while let Some(offset) = next_offset {
        if !visited.insert(offset) {
            return Err(invalid_data(format!(
                "E01 segment {segment_index} section chain cycles at offset {offset}"
            )));
        }
        let section = read_section(file, file_len, segment_index, offset)?;
        let done = section.kind == "done";
        next_offset = if done {
            None
        } else {
            valid_next_offset(section.next_offset, offset, file_len, segment_index)?
        };
        sections.push(section);
    }
    Ok(sections)
}

fn read_section(
    file: &mut std::fs::File,
    file_len: u64,
    segment_index: usize,
    offset: u64,
) -> io::Result<E01Section> {
    let descriptor_end = offset
        .checked_add(SECTION_DESCRIPTOR_SIZE)
        .ok_or_else(|| invalid_data("E01 section descriptor offset overflows u64"))?;
    if descriptor_end > file_len {
        return Err(invalid_data(format!(
            "E01 segment {segment_index} section descriptor at offset {offset} is truncated"
        )));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut descriptor = [0u8; SECTION_DESCRIPTOR_SIZE as usize];
    file.read_exact(&mut descriptor)?;
    let kind = String::from_utf8_lossy(&descriptor[0..16])
        .trim_end_matches('\0')
        .to_string();
    let next_offset = u64::from_le_bytes(descriptor[16..24].try_into().unwrap_or([0; 8]));
    let section_size = u64::from_le_bytes(descriptor[24..32].try_into().unwrap_or([0; 8]));
    let content = read_section_content(file, file_len, offset, section_size, &kind)?;
    Ok(E01Section {
        kind,
        segment_index,
        start_offset: offset,
        next_offset,
        content,
    })
}

fn read_section_content(
    file: &mut std::fs::File,
    file_len: u64,
    offset: u64,
    section_size: u64,
    section_type: &str,
) -> io::Result<Vec<u8>> {
    if !should_read_section_content(section_type) {
        return Ok(Vec::new());
    }
    if section_size < SECTION_DESCRIPTOR_SIZE {
        return Err(invalid_data(format!(
            "E01 {section_type} section at offset {offset} has invalid length {section_size}"
        )));
    }
    let read_size = section_size - SECTION_DESCRIPTOR_SIZE;
    if read_size > MAX_SECTION_CONTENT_BYTES {
        return Err(invalid_data(format!(
            "E01 {section_type} section content exceeds the {MAX_SECTION_CONTENT_BYTES} byte limit"
        )));
    }
    let data_start = offset
        .checked_add(SECTION_DESCRIPTOR_SIZE)
        .ok_or_else(|| invalid_data("E01 section content offset overflows u64"))?;
    let data_end = data_start
        .checked_add(read_size)
        .ok_or_else(|| invalid_data("E01 section content range overflows u64"))?;
    if data_end > file_len {
        return Err(invalid_data(format!(
            "E01 {section_type} section at offset {offset} extends beyond its segment"
        )));
    }
    let mut content = vec![0u8; read_size as usize];
    file.seek(SeekFrom::Start(data_start))?;
    file.read_exact(&mut content)?;
    Ok(content)
}

fn valid_next_offset(
    next: u64,
    current: u64,
    file_len: u64,
    segment_index: usize,
) -> io::Result<Option<u64>> {
    if next == 0 {
        return Ok(None);
    }
    let descriptor_end = next
        .checked_add(SECTION_DESCRIPTOR_SIZE)
        .ok_or_else(|| invalid_data("E01 next section offset overflows u64"))?;
    if next <= current || descriptor_end > file_len {
        return Err(invalid_data(format!(
            "E01 segment {segment_index} has invalid next section offset {next} after {current}"
        )));
    }
    Ok(Some(next))
}

fn geometry(segments: &[SegmentSections]) -> io::Result<E01Geometry> {
    let views = segments
        .iter()
        .flat_map(|segment| &segment.sections)
        .map(|section| (section.kind.clone(), section.content.clone()))
        .collect::<Vec<_>>();
    find_geometry(&views)
}

fn select_chunk_table(
    segments: &[SegmentSections],
    total_bytes: u64,
    chunk_bytes: u64,
) -> io::Result<Vec<(usize, u64, bool, u64)>> {
    let mut table = Vec::new();
    for segment in segments {
        table.extend(select_segment_chunk_table(segment)?);
    }
    let expected_chunks = total_bytes.div_ceil(chunk_bytes);
    if table.len() as u64 != expected_chunks {
        return Err(invalid_data(format!(
            "E01 chunk table count mismatch: expected {expected_chunks}, found {} across {} segment(s)",
            table.len(),
            segments.len()
        )));
    }
    Ok(table)
}

fn select_segment_chunk_table(
    segment: &SegmentSections,
) -> io::Result<Vec<(usize, u64, bool, u64)>> {
    let primary = build_chunk_table(&segment.sections, segment.file_len, "table");
    match primary {
        Ok(entries) if !entries.is_empty() => Ok(entries),
        Ok(_) => select_backup_table(segment, None),
        Err(primary_error) => select_backup_table(segment, Some(primary_error)),
    }
}

fn select_backup_table(
    segment: &SegmentSections,
    primary_error: Option<io::Error>,
) -> io::Result<Vec<(usize, u64, bool, u64)>> {
    match build_chunk_table(&segment.sections, segment.file_len, "table2") {
        Ok(entries) if !entries.is_empty() => Ok(entries),
        Ok(_) => Err(primary_error.unwrap_or_else(|| {
            invalid_data("E01 segment contains no usable table or table2 section")
        })),
        Err(backup_error) => Err(primary_error.unwrap_or(backup_error)),
    }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
