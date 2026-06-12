use crate::{
    datasource_service::{self, ImageFilesystemKind},
    file_service::mapping::mime_for_entry,
};
use domain::{EntryType, FileEntry, FileEntryId};
use evidence_core::{EvidenceReader, FileSystemReader, RawImageReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use transport::dto::{ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto};

const FILE_HANDLE_PREFIX: &str = "file:";

pub fn open_file_handle_real(conn: &Connection, file_id: &str) -> Result<ViewerHandleDto, String> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File not found".to_string())?;

    if entry.entry_type != EntryType::File {
        return Err("Cannot open a directory as a file".to_string());
    }

    Ok(ViewerHandleDto {
        handle_id: format!("{FILE_HANDLE_PREFIX}{}", entry.id.0),
        size: entry.size.unwrap_or(0),
        mime: mime_for_entry(&entry),
    })
}

pub fn read_file_range_for_case(
    conn: &Connection,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, String> {
    let mut request = request.clone();
    request.validate()?;
    let file_id = file_id_from_handle(&request.handle_id)?;
    let mut file = open_file_content_by_id(conn, &FileEntryId(file_id.to_string()))?;

    skip_reader_bytes(file.as_mut(), request.offset)?;
    let length = (request.length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    let mut bytes = vec![0u8; length];
    let read = file.read(&mut bytes).map_err(|e| e.to_string())?;
    bytes.truncate(read);

    Ok(ViewerRangeResponseDto {
        kind: "hex".into(),
        lines: format_hex_lines(request.offset, &bytes),
        encoding: None,
    })
}

pub fn read_file_range_real(_request: &ViewerRangeRequestDto) -> ViewerRangeResponseDto {
    empty_hex_response()
}

pub fn open_file_content_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
) -> Result<Box<dyn Read>, String> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File not found".to_string())?;

    open_file_content_for_entry(&repo, &entry)
}

pub fn read_file_header_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut reader = open_file_content_by_id(conn, file_id)?;
    let mut limited = reader.by_ref().take(max_bytes as u64);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

pub fn get_file_path_for_entry(conn: &Connection, file_id: &str) -> Result<PathBuf, String> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File not found".to_string())?;

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Data source not found".to_string())?;

    if kind == "logical_directory" {
        let root = PathBuf::from(&source_path)
            .canonicalize()
            .map_err(|e| format!("Cannot access data source root: {}", e))?;
        let relative_path = safe_relative_path(&entry.path)?;
        Ok(root.join(relative_path))
    } else {
        Err("File path only available for logical directories".to_string())
    }
}

fn open_file_content_for_entry(
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, String> {
    if entry.entry_type != EntryType::File {
        return Err("Cannot read a directory as a file".to_string());
    }

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Data source not found".to_string())?;
    let expected_partition_index = root_partition_index_for_entry(repo, entry);

    match kind.as_str() {
        "logical_directory" => open_logical_file(&source_path, entry),
        "e01" => open_e01_file(&source_path, entry, expected_partition_index),
        "raw" => open_raw_file(&source_path, entry, expected_partition_index),
        other => Err(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        )),
    }
}

fn open_logical_file(source_path: &str, entry: &FileEntry) -> Result<Box<dyn Read>, String> {
    let root = PathBuf::from(source_path)
        .canonicalize()
        .map_err(|e| format!("Cannot access data source root: {}", e))?;
    let relative_path = safe_relative_path(&entry.path)?;
    let full_path = root.join(relative_path);

    let mut check_path = PathBuf::new();
    for component in full_path.components() {
        check_path.push(component);
        if check_path.is_symlink() {
            return Err(format!(
                "Symlink detected in path at '{}' - rejected for security",
                check_path.display()
            ));
        }
    }

    let canonical = full_path
        .canonicalize()
        .map_err(|e| format!("Cannot access file '{}': {}", entry.path, e))?;

    if !canonical.starts_with(&root) {
        return Err("File path escapes data source root".to_string());
    }

    if !canonical.is_file() {
        return Err("File entry does not point to a regular file".to_string());
    }

    std::fs::File::open(canonical)
        .map(|file| Box::new(file) as Box<dyn Read>)
        .map_err(|e| e.to_string())
}

fn open_raw_file(
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, String> {
    let reader = RawImageReader::open(Path::new(source_path)).map_err(|e| e.to_string())?;
    open_image_file(entry, reader, expected_partition_index)
}

fn open_e01_file(
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, String> {
    let reader = E01Reader::open(Path::new(source_path)).map_err(|e| e.to_string())?;
    open_image_file(entry, reader, expected_partition_index)
}

fn open_image_file<R>(
    entry: &FileEntry,
    mut reader: R,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, String>
where
    R: EvidenceReader + Read + std::io::Seek + 'static,
{
    let probe =
        datasource_service::detect_image_filesystem(&mut reader).map_err(|e| e.to_string())?;
    if probe.candidates.is_empty() {
        let detail = if probe.warnings.is_empty() {
            "No supported NTFS/FAT filesystem detected".to_string()
        } else {
            probe.warnings.join("; ")
        };
        return Err(detail);
    }

    let source_path = reader.info().path.clone();
    let source_kind = reader.info().kind.clone();
    for candidate in probe.candidates {
        if let Some(expected_partition) = expected_partition_index {
            if candidate.partition_index != Some(expected_partition) {
                continue;
            }
        }

        let boxed_reader: Box<dyn EvidenceReader> = match source_kind.as_str() {
            "e01" => Box::new(E01Reader::open(&source_path).map_err(|e| e.to_string())?),
            _ => Box::new(RawImageReader::open(&source_path).map_err(|e| e.to_string())?),
        };

        let result = match candidate.kind {
            ImageFilesystemKind::Ntfs => {
                let fs = fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset)
                    .map_err(|e| e.to_string())?;
                fs.open_file(&entry.path)
                    .map_err(|e| format!("Cannot open NTFS file '{}': {}", entry.path, e))
            }
            ImageFilesystemKind::Fat => {
                let fs = fs_fat::FatReader::open(boxed_reader, candidate.offset)
                    .map_err(|e| e.to_string())?;
                fs.open_file(&entry.path)
                    .map_err(|e| format!("Cannot open FAT file '{}': {}", entry.path, e))
            }
            ImageFilesystemKind::BitLocker => Err(format!(
                "Cannot open '{}' from locked BitLocker partition",
                entry.path
            )),
        };

        if result.is_ok() {
            return result;
        }
    }

    Err(format!(
        "Cannot open image-backed file '{}' from any detected partition",
        entry.path
    ))
}

fn root_partition_index_for_entry(repo: &FileRepo<'_>, entry: &FileEntry) -> Option<usize> {
    if let Some(index) = mft_partition_index_from_entry_id(&entry.id.0) {
        return Some(index);
    }

    let mut current = entry.clone();
    while let Some(parent_id) = &current.parent_id {
        let parent = repo.find_by_id(parent_id).ok()??;
        current = parent;
    }

    current
        .name
        .strip_prefix("Partition ")?
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

pub(crate) fn mft_partition_index_from_entry_id(entry_id: &str) -> Option<usize> {
    let mut parts = entry_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("mft"), Some(partition), Some(_record), None) => partition.parse().ok(),
        _ => None,
    }
}

fn file_id_from_handle(handle_id: &str) -> Result<&str, String> {
    handle_id
        .strip_prefix(FILE_HANDLE_PREFIX)
        .filter(|file_id| !file_id.is_empty())
        .ok_or_else(|| "Invalid file handle".to_string())
}

pub fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("Empty file path".to_string());
    }

    let decoded = urlencoding_decode(path);
    if decoded != path
        && (decoded.contains("..") || decoded.contains('/') || decoded.contains('\\'))
    {
        return Err("URL-encoded traversal detected".to_string());
    }

    let mut safe = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::Normal(part) => {
                let s = part.to_str().ok_or("Invalid UTF-8 in path")?;
                if s.contains('\0') {
                    return Err("Null byte in path".to_string());
                }
                if is_windows_reserved_name(s) {
                    return Err(format!("Reserved name: {}", s));
                }
                safe.push(part);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err("Unsafe file path in catalog".to_string())
            }
        }
    }

    if safe.as_os_str().len() > infrastructure::constants::MAX_PATH_LENGTH {
        return Err("Path too long".to_string());
    }

    Ok(safe)
}

fn urlencoding_decode(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push(c);
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn is_windows_reserved_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let upper = name.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or(&upper);
    RESERVED.contains(&stem)
}

pub fn skip_reader_bytes(reader: &mut dyn Read, mut remaining: u64) -> Result<(), String> {
    let mut buffer = vec![0u8; 65536];
    while remaining > 0 {
        let chunk_len = remaining.min(buffer.len() as u64) as usize;
        let read = reader
            .read(&mut buffer[..chunk_len])
            .map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("Read offset exceeds file size".to_string());
        }
        remaining -= read as u64;
    }
    Ok(())
}

fn format_hex_lines(base_offset: u64, bytes: &[u8]) -> Vec<String> {
    let line_count = bytes.len().div_ceil(16);
    let mut result = Vec::with_capacity(line_count);

    for (line_idx, chunk) in bytes.chunks(16).enumerate() {
        let offset = base_offset + (line_idx * 16) as u64;
        let mut line = String::with_capacity(8 + 2 + chunk.len() * 4);

        use std::fmt::Write;
        let _ = write!(line, "{offset:08X}");
        line.push_str("  ");

        for (i, byte) in chunk.iter().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            let _ = write!(line, "{byte:02X}");
        }

        result.push(line);
    }
    result
}

fn empty_hex_response() -> ViewerRangeResponseDto {
    ViewerRangeResponseDto {
        kind: "hex".into(),
        lines: Vec::new(),
        encoding: None,
    }
}
