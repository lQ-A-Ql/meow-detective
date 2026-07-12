use std::collections::{HashSet, VecDeque};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use evidence_core::ReaderInfo;

use super::{
    build_chunk_table, build_segment_path, find_geometry, should_read_section_content, E01Reader,
    SECTION_DESCRIPTOR_SIZE,
};

type Section = (String, u64, u64, Vec<u8>);

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut segment_files = open_segment_files(path)?;
        let file_len = verify_header(&mut segment_files[0])?;
        let sections = read_sections(&mut segment_files[0], file_len)?;
        let (total_bytes, chunk_size_sectors) = geometry(&sections, file_len)?;
        let chunk_table =
            select_chunk_table(&sections, &segment_files, total_bytes, chunk_size_sectors)?;
        Ok(Self {
            info: ReaderInfo {
                path: path.to_path_buf(),
                size: total_bytes,
                kind: "e01".into(),
            },
            total_bytes,
            chunk_size_sectors,
            chunk_table: Arc::new(chunk_table),
            segment_files,
            cursor: 0,
            chunk_cache: VecDeque::new(),
            chunk_cache_bytes: 0,
            last_chunk_read: None,
        })
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

fn verify_header(file: &mut std::fs::File) -> io::Result<u64> {
    let file_len = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; 13];
    file.read_exact(&mut header)?;
    if &header[0..3] != b"EVF" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not EWF"));
    }
    Ok(file_len)
}

fn read_sections(file: &mut std::fs::File, file_len: u64) -> io::Result<Vec<Section>> {
    let mut visited = HashSet::new();
    let mut next_offset = 13u64;
    let mut sections = Vec::new();
    while next_offset > 0 && next_offset < file_len {
        if !visited.insert(next_offset) {
            tracing::warn!(
                "E01: cycle detected in section chain at offset 0x{:X}, stopping",
                next_offset
            );
            break;
        }
        let Some(section) = read_section(file, file_len, next_offset)? else {
            break;
        };
        let done = section.0 == "done";
        next_offset = valid_next_offset(section.2, file_len);
        sections.push(section);
        if done {
            break;
        }
    }
    Ok(sections)
}

fn read_section(
    file: &mut std::fs::File,
    file_len: u64,
    offset: u64,
) -> io::Result<Option<Section>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut descriptor = [0u8; SECTION_DESCRIPTOR_SIZE as usize];
    if file.read_exact(&mut descriptor).is_err() {
        return Ok(None);
    }
    let section_type = String::from_utf8_lossy(&descriptor[0..16])
        .trim_end_matches('\0')
        .to_string();
    let next = u64::from_le_bytes(descriptor[16..24].try_into().unwrap_or([0; 8]));
    let section_size = u64::from_le_bytes(descriptor[24..32].try_into().unwrap_or([0; 8]));
    let content = read_section_content(file, file_len, offset, next, section_size, &section_type)?;
    Ok(Some((section_type, offset, next, content)))
}

fn read_section_content(
    file: &mut std::fs::File,
    file_len: u64,
    offset: u64,
    next: u64,
    section_size: u64,
    section_type: &str,
) -> io::Result<Vec<u8>> {
    if !should_read_section_content(section_type) {
        return Ok(Vec::new());
    }
    let data_start = offset.saturating_add(SECTION_DESCRIPTOR_SIZE);
    let size_from_section = section_size.saturating_sub(SECTION_DESCRIPTOR_SIZE);
    let size_from_next = if next > data_start && next <= file_len {
        next - data_start
    } else {
        0
    };
    let read_size = bounded_section_size(size_from_section, size_from_next, file_len, data_start);
    let mut content = vec![0u8; read_size as usize];
    if read_size > 0 {
        file.seek(SeekFrom::Start(data_start))?;
        file.read_exact(&mut content)?;
    }
    Ok(content)
}

fn bounded_section_size(
    size_from_section: u64,
    size_from_next: u64,
    file_len: u64,
    data_start: u64,
) -> u64 {
    let candidate = if size_from_section > 0 && size_from_next > 0 {
        size_from_section.min(size_from_next)
    } else {
        size_from_section.max(size_from_next)
    };
    candidate
        .min(10_000_000)
        .min(file_len.saturating_sub(data_start))
}

fn valid_next_offset(next: u64, file_len: u64) -> u64 {
    if next > 0 && next < file_len {
        next
    } else {
        0
    }
}

fn geometry(sections: &[Section], file_len: u64) -> io::Result<(u64, u32)> {
    let views = sections
        .iter()
        .map(|(kind, _, _, content)| (kind.clone(), content.clone()))
        .collect::<Vec<_>>();
    let (sectors, chunk_size) = find_geometry(&views, file_len)?;
    Ok((sectors * 512, if chunk_size > 0 { chunk_size } else { 64 }))
}

fn select_chunk_table(
    sections: &[Section],
    segment_files: &[std::fs::File],
    total_bytes: u64,
    chunk_size_sectors: u32,
) -> io::Result<Vec<(usize, u64, bool, u64)>> {
    let segment_sizes = segment_files
        .iter()
        .map(|file| file.metadata().map(|metadata| metadata.len()).unwrap_or(0))
        .collect::<Vec<_>>();
    let chunk_bytes = u64::from(chunk_size_sectors) * 512;
    let expected_chunks = total_bytes.div_ceil(chunk_bytes);
    let mut table = build_chunk_table(sections, &segment_sizes, "table");
    if table.len() as u64 != expected_chunks {
        let fallback = build_chunk_table(sections, &segment_sizes, "table2");
        if fallback.len() as u64 == expected_chunks {
            table = fallback;
        }
    }
    if table.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no usable chunk table found",
        ));
    }
    Ok(table)
}
