use crate::file_service::{mapping::mime_for_entry, FileServiceError};
use domain::{EntryType, FileEntry, FileEntryId};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::partition_repo::PartitionRepo;
use rusqlite::Connection;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use transport::dto::{ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto};

const FILE_HANDLE_PREFIX: &str = "file:";

/// Maximum number of concurrently cached parsed E01 readers.
/// Cache hits reuse the `Arc<chunk_table>` via `E01Reader::re_open`,
/// opening fresh segment file handles without re-parsing headers.
const E01_READER_CACHE_MAX_SIZE: usize = 4;

struct E01ReaderCache {
    max_size: usize,
    paths: VecDeque<PathBuf>,
    readers: HashMap<PathBuf, E01Reader>,
}

impl E01ReaderCache {
    fn new(max_size: usize) -> Self {
        Self {
            max_size,
            paths: VecDeque::with_capacity(max_size),
            readers: HashMap::with_capacity(max_size),
        }
    }

    fn get_or_open(&mut self, source_path: &Path) -> std::io::Result<E01Reader> {
        // Cache hit: re-open fresh file handles, share Arc<chunk_table>
        if let Some(cached) = self.readers.get(source_path) {
            // Update LRU: move to most-recently-used end
            if let Some(pos) = self.paths.iter().position(|p| p == source_path) {
                self.paths.remove(pos);
            }
            self.paths.push_back(source_path.to_path_buf());
            return cached.re_open(source_path);
        }

        // Cache miss: fully parse from disk
        let reader = E01Reader::open(source_path)?;

        // Evict oldest if full
        while self.paths.len() >= self.max_size {
            if let Some(evict_path) = self.paths.pop_front() {
                self.readers.remove(&evict_path);
            }
        }

        self.paths.push_back(source_path.to_path_buf());
        self.readers.insert(source_path.to_path_buf(), reader);

        // Re-open for the caller with fresh handles
        self.readers.get(source_path).unwrap().re_open(source_path)
    }
}

static E01_READER_CACHE: std::sync::LazyLock<std::sync::Mutex<E01ReaderCache>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(E01ReaderCache::new(E01_READER_CACHE_MAX_SIZE))
    });

pub fn clear_e01_reader_cache() {
    if let Ok(mut cache) = E01_READER_CACHE.lock() {
        cache.paths.clear();
        cache.readers.clear();
    }
}

fn open_e01_reader_cached(source_path: &Path) -> std::io::Result<E01Reader> {
    E01_READER_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_or_open(source_path)
}

pub fn open_file_handle_real(
    conn: &Connection,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;

    if entry.entry_type != EntryType::File {
        return Err(FileServiceError::invalid_input(
            "Cannot open a directory as a file",
        ));
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
) -> Result<ViewerRangeResponseDto, FileServiceError> {
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let file_id = file_id_from_handle(&request.handle_id)?;
    let mut file = open_file_content_by_id(conn, &FileEntryId(file_id.to_string()))?;

    skip_reader_bytes(file.as_mut(), request.offset)?;
    let length = (request.length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    let mut bytes = vec![0u8; length];
    let read = file.read(&mut bytes)?;
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
) -> Result<Box<dyn Read>, FileServiceError> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(file_id)?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;

    open_file_content_for_entry(conn, &repo, &entry)
}

pub fn read_file_header_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Vec<u8>, FileServiceError> {
    let mut reader = open_file_content_by_id(conn, file_id)?;
    let mut limited = reader.by_ref().take(max_bytes as u64);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn get_file_path_for_entry(
    conn: &Connection,
    file_id: &str,
) -> Result<PathBuf, FileServiceError> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;

    if kind == "logical_directory" {
        let root = PathBuf::from(&source_path).canonicalize()?;
        let relative_path = safe_relative_path(&entry.path)?;
        Ok(root.join(relative_path))
    } else {
        Err(FileServiceError::other(
            "File path only available for logical directories",
        ))
    }
}

fn open_file_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    if entry.entry_type != EntryType::File {
        return Err(FileServiceError::invalid_input(
            "Cannot read a directory as a file",
        ));
    }

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index = root_partition_index_for_entry(repo, entry);

    match kind.as_str() {
        "logical_directory" => open_logical_file(&source_path, entry),
        "e01" => open_e01_file(conn, &source_path, entry, expected_partition_index),
        "raw" => open_raw_file(&source_path, entry, expected_partition_index),
        other => Err(FileServiceError::other(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        ))),
    }
}

fn open_logical_file(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    let root = PathBuf::from(source_path).canonicalize()?;
    let relative_path = safe_relative_path(&entry.path)?;
    let full_path = root.join(relative_path);

    let mut check_path = PathBuf::new();
    for component in full_path.components() {
        check_path.push(component);
        if check_path.is_symlink() {
            return Err(FileServiceError::other(format!(
                "Symlink detected in path at '{}' - rejected for security",
                check_path.display()
            )));
        }
    }

    let canonical = full_path.canonicalize()?;

    if !canonical.starts_with(&root) {
        return Err(FileServiceError::path_traversal(
            "File path escapes data source root",
        ));
    }

    if !canonical.is_file() {
        return Err(FileServiceError::other(
            "File entry does not point to a regular file",
        ));
    }

    Ok(Box::new(std::fs::File::open(canonical)?) as Box<dyn Read>)
}

fn open_raw_file(
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    let reader = evidence_core::RawImageReader::open(Path::new(source_path))?;
    // RAW 镜像：使用简单的 MBR/GPT 探测（不需要缓存的分区表）
    open_raw_image_file(entry, reader, expected_partition_index)
}

fn open_raw_image_file<R>(
    entry: &FileEntry,
    mut reader: R,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError>
where
    R: evidence_core::EvidenceReader + Read + std::io::Seek + 'static,
{
    let probe = crate::datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|e| FileServiceError::other(format!("Failed to detect RAW filesystem: {e}")))?;
    if probe.candidates.is_empty() {
        return Err(FileServiceError::other(
            "No supported filesystem detected in RAW image",
        ));
    }
    let source_path = reader.info().path.clone();
    let candidates =
        crate::datasource_service::assign_effective_partition_indices(&probe.candidates);
    for (ci, candidate) in probe.candidates.iter().enumerate() {
        let eff = crate::datasource_service::effective_partition_index(candidate, ci, &candidates);
        if let Some(expected) = expected_partition_index {
            if eff != expected {
                continue;
            }
        }
        let boxed: Box<dyn evidence_core::EvidenceReader> =
            Box::new(evidence_core::RawImageReader::open(&source_path)?);
        match candidate.kind {
            crate::datasource_service::ImageFilesystemKind::Ntfs => {
                if let Ok(fs) = fs_ntfs::NtfsReader::open(boxed, candidate.offset) {
                    if let Ok(r) = fs.open_file(&entry.path) {
                        return Ok(r);
                    }
                }
            }
            crate::datasource_service::ImageFilesystemKind::Fat => {
                if let Ok(fs) = fs_fat::FatReader::open(boxed, candidate.offset) {
                    if let Ok(r) = fs.open_file(&entry.path) {
                        return Ok(r);
                    }
                }
            }
            _ => {}
        }
    }
    Err(FileServiceError::other(format!(
        "Cannot open RAW image file '{}' from any partition",
        entry.path
    )))
}

fn open_e01_file(
    conn: &Connection,
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    // 查询导入时已存储的分区元数据
    let part_repo = PartitionRepo::new(conn);
    let partitions = part_repo
        .find_by_data_source(&entry.data_source_id.0)
        .map_err(|e| FileServiceError::other(format!("Failed to query partitions: {e}")))?;

    if partitions.is_empty() {
        return Err(FileServiceError::other(
            "No partition metadata found for this data source. Re-import the E01 image.",
        ));
    }

    // 如果路径只是裸文件名，说明 import 时父链重构失败
    if !entry.path.contains('/') && !entry.path.contains('\\') {
        return Err(FileServiceError::other(format!(
            "Cannot preview '{}': path reconstruction did not resolve the parent directory. Re-import.",
            entry.path
        )));
    }

    // 收集候选分区：优先匹配 expected_partition_index，否则尝试所有
    let candidates_to_try: Vec<
        &persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord,
    > = if let Some(expected) = expected_partition_index {
        partitions
            .iter()
            .filter(|p| p.partition_index as usize == expected)
            .collect()
    } else {
        Vec::new()
    };

    let candidates_to_try = if candidates_to_try.is_empty() {
        partitions
            .iter()
            .filter(|p| p.status != "EncryptedBitLocker")
            .collect()
    } else {
        candidates_to_try
    };

    for target in &candidates_to_try {
        if target.status == "EncryptedBitLocker" {
            continue;
        }
        let fs_kind = target.filesystem.as_deref().unwrap_or(&target.kind_label);

        let reader = open_e01_reader_cached(Path::new(source_path))?;
        let boxed_reader: Box<dyn evidence_core::EvidenceReader> = Box::new(reader);

        let result = match fs_kind {
            "NTFS" => match fs_ntfs::NtfsReader::open(boxed_reader, target.offset) {
                Ok(fs) => fs
                    .open_file(&entry.path)
                    .or_else(|_| fs.open_file(&entry.id.0)),
                Err(e) => {
                    tracing::warn!(
                        path = %entry.path,
                        partition = %target.name,
                        offset = %target.offset,
                        error = %e,
                        "E01 NTFS open failed"
                    );
                    continue;
                }
            },
            "FAT" | "FAT32" | "FAT16" | "FAT12" => {
                match fs_fat::FatReader::open(boxed_reader, target.offset) {
                    Ok(fs) => fs.open_file(&entry.path),
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path,
                            partition = %target.name,
                            offset = %target.offset,
                            error = %e,
                            "E01 FAT open failed"
                        );
                        continue;
                    }
                }
            }
            _ => continue,
        };

        match &result {
            Ok(_) => return result.map_err(|e| FileServiceError::other(format!("{e}"))),
            Err(e) => {
                tracing::warn!(
                    path = %entry.path,
                    partition = %target.name,
                    kind = %fs_kind,
                    error = %e,
                    "E01 file not found on partition"
                );
            }
        }
    }

    Err(FileServiceError::other(format!(
        "Cannot open image-backed file '{}' from any partition",
        entry.path
    )))
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

fn file_id_from_handle(handle_id: &str) -> Result<&str, FileServiceError> {
    handle_id
        .strip_prefix(FILE_HANDLE_PREFIX)
        .filter(|file_id| !file_id.is_empty())
        .ok_or_else(|| FileServiceError::invalid_input("Invalid file handle"))
}

pub fn safe_relative_path(path: &str) -> Result<PathBuf, FileServiceError> {
    if path.is_empty() {
        return Err(FileServiceError::invalid_input("Empty file path"));
    }

    let decoded = urlencoding_decode(path);
    if decoded != path
        && (decoded.contains("..") || decoded.contains('/') || decoded.contains('\\'))
    {
        return Err(FileServiceError::path_traversal(
            "URL-encoded traversal detected",
        ));
    }

    let mut safe = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::Normal(part) => {
                let s = part
                    .to_str()
                    .ok_or_else(|| FileServiceError::path_traversal("Invalid UTF-8 in path"))?;
                if s.contains('\0') {
                    return Err(FileServiceError::path_traversal("Null byte in path"));
                }
                if is_windows_reserved_name(s) {
                    return Err(FileServiceError::path_traversal(format!(
                        "Reserved name: {}",
                        s
                    )));
                }
                safe.push(part);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(FileServiceError::path_traversal(
                    "Unsafe file path in catalog",
                ))
            }
        }
    }

    if safe.as_os_str().len() > infrastructure::constants::MAX_PATH_LENGTH {
        return Err(FileServiceError::path_traversal("Path too long"));
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

pub fn skip_reader_bytes(
    reader: &mut dyn Read,
    mut remaining: u64,
) -> Result<(), FileServiceError> {
    let mut buffer = vec![0u8; 65536];
    while remaining > 0 {
        let chunk_len = remaining.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..chunk_len])?;
        if read == 0 {
            return Err(FileServiceError::other("Read offset exceeds file size"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};

    fn make_temp_e01() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("cache_test.E01");
        // Write a minimal single-chunk E01 so the reader can be opened.
        write_tiny_e01(&path).unwrap();
        (dir, path)
    }

    fn write_tiny_e01(path: &std::path::Path) -> std::io::Result<()> {
        let chunk_sectors: u32 = 8;
        let sectors = chunk_sectors as u64;
        let chunk_bytes = (chunk_sectors * 512) as usize;

        let mut f = std::fs::File::create(path)?;
        // EVF file header (13 bytes)
        f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")?;

        let mut vol = vec![0u8; 36];
        vol[12..16].copy_from_slice(&chunk_sectors.to_le_bytes());
        vol[16..24].copy_from_slice(&sectors.to_le_bytes());

        let volume_desc_offset = 13u64;
        let table_desc_offset = volume_desc_offset + 76 + vol.len() as u64;
        let table_len = 24 + 4 + 4; // 1 chunk entry + padding
        let done_desc_offset = table_desc_offset + 76 + table_len as u64;
        let chunk0_offset = done_desc_offset + 76;

        // volume section
        f.write_all(&section_desc(
            "volume",
            table_desc_offset,
            76 + vol.len() as u64,
        ))?;
        f.write_all(&vol)?;

        // table section (1 chunk)
        let mut table = vec![0u8; table_len];
        table[0..4].copy_from_slice(&1u32.to_le_bytes()); // 1 entry
        table[8..16].copy_from_slice(&chunk0_offset.to_le_bytes()); // base offset
        table[24..28].copy_from_slice(&0u32.to_le_bytes()); // rel offset 0
        f.write_all(&section_desc(
            "table",
            done_desc_offset,
            76 + table.len() as u64,
        ))?;
        f.write_all(&table)?;

        // done section
        f.write_all(&section_desc("done", 0, 0))?;

        // chunk data
        let marker = b"E01-CACHE-TEST";
        let mut chunk = vec![0u8; chunk_bytes];
        chunk[..marker.len()].copy_from_slice(marker);
        f.write_all(&chunk)?;
        f.flush()
    }

    fn section_desc(stype: &str, next: u64, size: u64) -> [u8; 76] {
        let mut desc = [0u8; 76];
        let bytes = stype.as_bytes();
        desc[0..bytes.len().min(16)].copy_from_slice(&bytes[..bytes.len().min(16)]);
        desc[16..24].copy_from_slice(&next.to_le_bytes());
        desc[24..32].copy_from_slice(&size.to_le_bytes());
        desc
    }

    #[test]
    fn fresh_e01_reader_opens_successfully() {
        let (_dir, path) = make_temp_e01();
        let mut reader = E01Reader::open(&path).unwrap();
        let mut buf = [0u8; 14];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"E01-CACHE-TEST");
    }

    #[test]
    fn fresh_e01_readers_have_independent_positions() {
        let (_dir, path) = make_temp_e01();
        let mut reader1 = E01Reader::open(&path).unwrap();
        let mut reader2 = E01Reader::open(&path).unwrap();

        reader1.seek(SeekFrom::Start(0)).unwrap();
        reader2.seek(SeekFrom::Start(4)).unwrap();
        let mut b1 = [0u8; 4];
        let mut b2 = [0u8; 4];
        reader1.read_exact(&mut b1).unwrap();
        reader2.read_exact(&mut b2).unwrap();
        assert_eq!(&b1, b"E01-");
        assert_eq!(&b2, b"CACH");
    }
}
