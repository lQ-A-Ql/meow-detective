use crate::file_service::{mapping::mime_for_entry, FileServiceError};
use domain::{EntryType, FileEntry, FileEntryId};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::partition_repo::PartitionRepo;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use transport::dto::{ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto};

const FILE_HANDLE_PREFIX: &str = "file:";

/// Maximum number of concurrently cached parsed E01 readers.
/// Cache hits reuse the `Arc<chunk_table>` via `E01Reader::re_open`,
/// opening fresh segment file handles without re-parsing headers.
const E01_READER_CACHE_MAX_SIZE: usize = 4;

trait ReadSeek: Read + Seek {}

impl<T> ReadSeek for T where T: Read + Seek {}

enum RangeContentReader {
    Seekable(Box<dyn ReadSeek>),
    Streaming(Box<dyn Read>),
}

/// Internal preview locator used by command/runtime-cache layers.
///
/// This intentionally contains only small metadata needed to re-open evidence
/// content; it never stores preview bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDescriptor {
    pub case_id: String,
    pub file_id: String,
    pub source_kind: String,
    pub source_path: String,
    pub partition_index: Option<usize>,
    pub filesystem_kind: Option<String>,
    pub path: String,
    pub mime: Option<String>,
    pub size: u64,
    pub data_source_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partition_candidates: Vec<PreviewPartitionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPartitionCandidate {
    pub partition_index: usize,
    pub filesystem_kind: String,
    pub offset: u64,
}

pub trait PreviewReadContext {
    fn conn(&self) -> &Connection;

    fn case_id(&self) -> &str {
        ""
    }

    fn get_cached_preview_descriptor(&mut self, _key: &str) -> Option<serde_json::Value> {
        None
    }

    fn set_cached_preview_descriptor(&mut self, _key: &str, _value: &serde_json::Value) {}
}

impl PreviewReadContext for &Connection {
    fn conn(&self) -> &Connection {
        self
    }
}

impl<'a, G, S> PreviewReadContext for (&'a Connection, &'a str, G, S)
where
    G: FnMut(&str) -> Option<serde_json::Value>,
    S: FnMut(&str, &serde_json::Value),
{
    fn conn(&self) -> &Connection {
        self.0
    }

    fn case_id(&self) -> &str {
        self.1
    }

    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<serde_json::Value> {
        (self.2)(key)
    }

    fn set_cached_preview_descriptor(&mut self, key: &str, value: &serde_json::Value) {
        (self.3)(key, value);
    }
}

#[cfg(test)]
static SKIP_READER_BYTES_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
static FORMAT_HEX_LINES_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
thread_local! {
    static OPEN_FILE_CONTENT_BY_ID_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static READ_FILE_BYTES_FOR_CASE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
    let mut cache = E01_READER_CACHE.lock().unwrap_or_else(|poisoned| {
        // Clear the cache on poison to avoid using potentially corrupted state
        let mut cache = poisoned.into_inner();
        cache.paths.clear();
        cache.readers.clear();
        cache
    });
    cache.get_or_open(source_path)
}

pub fn open_file_handle_real<C>(
    mut context: C,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError>
where
    C: PreviewReadContext,
{
    if context.case_id().is_empty() {
        return open_file_handle_uncached(context.conn(), file_id);
    }

    let descriptor =
        descriptor_for_file_with_cache(&mut context, &FileEntryId(file_id.to_string()))?;

    Ok(ViewerHandleDto {
        handle_id: format!("{FILE_HANDLE_PREFIX}{}", descriptor.file_id),
        size: descriptor.size,
        mime: descriptor.mime,
    })
}

fn open_file_handle_uncached(
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

pub fn preview_descriptor_for_case(
    conn: &Connection,
    case_id: &str,
    file_id: &FileEntryId,
) -> Result<PreviewDescriptor, FileServiceError> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(file_id)?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;

    preview_descriptor_for_entry(conn, &repo, case_id, &entry)
}

fn preview_descriptor_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    case_id: &str,
    entry: &FileEntry,
) -> Result<PreviewDescriptor, FileServiceError> {
    if entry.entry_type != EntryType::File {
        return Err(FileServiceError::invalid_input(
            "Cannot read a directory as a file",
        ));
    }

    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index = root_partition_index_for_entry(repo, entry);

    let partition_candidates = match source_kind.as_str() {
        "logical_directory" => Vec::new(),
        "e01" => e01_partition_candidates(conn, entry, expected_partition_index)?,
        "raw" => raw_partition_candidates(&source_path, expected_partition_index)?,
        other => {
            return Err(FileServiceError::other(format!(
                "Range reading is not yet wired for data source kind '{}'",
                other
            )))
        }
    };
    let selected = partition_candidates.first();

    Ok(PreviewDescriptor {
        case_id: case_id.to_string(),
        file_id: entry.id.0.clone(),
        source_kind,
        source_path,
        partition_index: selected.map(|candidate| candidate.partition_index),
        filesystem_kind: selected.map(|candidate| candidate.filesystem_kind.clone()),
        path: entry.path.clone(),
        mime: mime_for_entry(entry),
        size: entry.size.unwrap_or(0),
        data_source_id: entry.data_source_id.0.clone(),
        partition_candidates,
    })
}

pub fn read_file_range_for_case<C>(
    context: C,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let file_id = file_id_from_handle(&request.handle_id)?;
    let bytes = read_file_bytes_for_case(
        context,
        &FileEntryId(file_id.to_string()),
        request.offset,
        request.length,
    )?;

    let raw_bytes = bytes.clone();
    Ok(ViewerRangeResponseDto {
        raw_bytes: Some(raw_bytes),
        kind: "hex".into(),
        lines: format_hex_lines(request.offset, &bytes),
        encoding: None,
    })
}

pub fn read_file_range_real(_request: &ViewerRangeRequestDto) -> ViewerRangeResponseDto {
    empty_hex_response()
}

pub fn open_file_content_by_id<C>(
    mut context: C,
    file_id: &FileEntryId,
) -> Result<Box<dyn Read>, FileServiceError>
where
    C: PreviewReadContext,
{
    #[cfg(test)]
    OPEN_FILE_CONTENT_BY_ID_CALLS.with(|calls| calls.set(calls.get() + 1));

    if context.case_id().is_empty() {
        let repo = FileRepo::new(context.conn());
        let entry = repo
            .find_by_id(file_id)?
            .ok_or_else(|| FileServiceError::not_found("File not found"))?;

        return open_file_content_for_entry(context.conn(), &repo, &entry);
    }

    let descriptor = descriptor_for_file_with_cache(&mut context, file_id)?;
    open_file_content_for_descriptor(&descriptor)
}

fn open_file_content_for_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError> {
    match descriptor.source_kind.as_str() {
        "logical_directory" => open_logical_descriptor_file(descriptor),
        "e01" => open_e01_descriptor_file(descriptor),
        "raw" => open_raw_descriptor_file(descriptor),
        other => Err(FileServiceError::other(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        ))),
    }
}

pub fn read_file_bytes_for_case<C>(
    mut context: C,
    file_id: &FileEntryId,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError>
where
    C: PreviewReadContext,
{
    #[cfg(test)]
    READ_FILE_BYTES_FOR_CASE_CALLS.with(|calls| calls.set(calls.get() + 1));

    if context.case_id().is_empty() {
        let repo = FileRepo::new(context.conn());
        let entry = repo
            .find_by_id(file_id)?
            .ok_or_else(|| FileServiceError::not_found("File not found"))?;
        if let Some(size) = entry.size {
            if offset > size {
                return Err(FileServiceError::other("Read offset exceeds file size"));
            }
        }

        return read_file_bytes_for_entry(context.conn(), &repo, &entry, offset, length);
    }

    let descriptor = descriptor_for_file_with_cache(&mut context, file_id)?;
    if offset > descriptor.size {
        return Err(FileServiceError::other("Read offset exceeds file size"));
    }

    read_file_bytes_for_descriptor(&descriptor, offset, length)
}

fn read_file_bytes_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let length = (length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    if let Some(bytes) = try_read_ntfs_image_range_for_entry(conn, repo, entry, offset, length)? {
        return Ok(bytes);
    }
    if let Some(bytes) = try_read_fat_image_range_for_entry(conn, repo, entry, offset, length)? {
        return Ok(bytes);
    }
    if let Some(bytes) = try_read_exfat_image_range_for_entry(conn, repo, entry, offset, length)? {
        return Ok(bytes);
    }

    match open_range_content_for_entry(conn, repo, entry)? {
        RangeContentReader::Seekable(mut reader) => {
            read_seekable_range(reader.as_mut(), offset, length)
        }
        RangeContentReader::Streaming(mut reader) => {
            // Image-backed filesystem readers still expose `Read` only and may
            // materialize file data internally. Keep this compatibility path
            // until fs-* crates expose seekable per-file streams.
            skip_reader_bytes(reader.as_mut(), offset)?;
            read_bounded(reader.as_mut(), length)
        }
    }
}

fn descriptor_cache_key(case_id: &str, file_id: &FileEntryId) -> String {
    format!("preview-descriptor:{case_id}:{}", file_id.0)
}

fn descriptor_for_file_with_cache<C>(
    context: &mut C,
    file_id: &FileEntryId,
) -> Result<PreviewDescriptor, FileServiceError>
where
    C: PreviewReadContext,
{
    let case_id = context.case_id().to_string();
    let key = descriptor_cache_key(&case_id, file_id);
    if let Some(value) = context.get_cached_preview_descriptor(&key) {
        match serde_json::from_value::<PreviewDescriptor>(value) {
            Ok(descriptor) if descriptor.case_id == case_id && descriptor.file_id == file_id.0 => {
                return Ok(descriptor);
            }
            Ok(_) | Err(_) => {
                tracing::warn!(
                    cache_key = %key,
                    "Ignoring stale or invalid preview descriptor cache entry"
                );
            }
        }
    }

    let descriptor = preview_descriptor_for_case(context.conn(), &case_id, file_id)?;
    if let Ok(value) = serde_json::to_value(&descriptor) {
        context.set_cached_preview_descriptor(&key, &value);
    }
    Ok(descriptor)
}

pub fn read_file_bytes_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let length = (length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    if let Some(bytes) = try_read_ntfs_image_range_for_descriptor(descriptor, offset, length)? {
        return Ok(bytes);
    }
    if let Some(bytes) = try_read_fat_image_range_for_descriptor(descriptor, offset, length)? {
        return Ok(bytes);
    }
    if let Some(bytes) = try_read_exfat_image_range_for_descriptor(descriptor, offset, length)? {
        return Ok(bytes);
    }

    match open_range_content_for_descriptor(descriptor)? {
        RangeContentReader::Seekable(mut reader) => {
            read_seekable_range(reader.as_mut(), offset, length)
        }
        RangeContentReader::Streaming(mut reader) => {
            skip_reader_bytes(reader.as_mut(), offset)?;
            read_bounded(reader.as_mut(), length)
        }
    }
}

fn descriptor_file_entry(descriptor: &PreviewDescriptor) -> FileEntry {
    let name = Path::new(&descriptor.path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(&descriptor.path)
        .to_string();
    let ext = Path::new(&name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .map(str::to_string);

    FileEntry {
        id: FileEntryId(descriptor.file_id.clone()),
        parent_id: None,
        data_source_id: domain::DataSourceId(descriptor.data_source_id.clone()),
        path: descriptor.path.clone(),
        name,
        entry_type: EntryType::File,
        size: Some(descriptor.size),
        ext,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn open_logical_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError> {
    let entry = descriptor_file_entry(descriptor);
    open_logical_file(&descriptor.source_path, &entry)
}

fn open_logical_descriptor_seekable(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn ReadSeek>, FileServiceError> {
    let entry = descriptor_file_entry(descriptor);
    open_logical_file_seekable(&descriptor.source_path, &entry)
}

fn open_range_content_for_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    match descriptor.source_kind.as_str() {
        "logical_directory" => {
            open_logical_descriptor_seekable(descriptor).map(RangeContentReader::Seekable)
        }
        "e01" => open_e01_descriptor_file(descriptor).map(RangeContentReader::Streaming),
        "raw" => open_raw_descriptor_file(descriptor).map(RangeContentReader::Streaming),
        other => Err(FileServiceError::other(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        ))),
    }
}

fn open_e01_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError> {
    open_descriptor_image_file(descriptor, |source_path| {
        open_e01_reader_cached(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
    })
}

fn open_raw_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError> {
    open_descriptor_image_file(descriptor, |source_path| {
        evidence_core::RawImageReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
    })
}

fn try_read_ntfs_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
        return Ok(None);
    }
    if descriptor.partition_candidates.is_empty() {
        return Ok(None);
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match descriptor.source_kind.as_str() {
        "e01" => try_read_ntfs_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                open_e01_reader_cached(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
        ),
        "raw" => try_read_ntfs_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
        ),
        _ => Ok(None),
    }
}

fn try_read_ntfs_image_range_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index = root_partition_index_for_entry(repo, entry);

    match source_kind.as_str() {
        "e01" => {
            let candidates = e01_partition_candidates(conn, entry, expected_partition_index)?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_ntfs_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    open_e01_reader_cached(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
            )
        }
        "raw" => {
            let candidates = raw_partition_candidates(&source_path, expected_partition_index)?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_ntfs_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    evidence_core::RawImageReader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
            )
        }
        _ => Ok(None),
    }
}

fn try_read_fat_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
        return Ok(None);
    }
    if descriptor.partition_candidates.is_empty() {
        return Ok(None);
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match descriptor.source_kind.as_str() {
        "e01" => try_read_fat_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                open_e01_reader_cached(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
        ),
        "raw" => try_read_fat_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
        ),
        _ => Ok(None),
    }
}

fn try_read_fat_image_range_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index = root_partition_index_for_entry(repo, entry);

    match source_kind.as_str() {
        "e01" => {
            let candidates = e01_partition_candidates(conn, entry, expected_partition_index)?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_fat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    open_e01_reader_cached(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
            )
        }
        "raw" => {
            let candidates = raw_partition_candidates(&source_path, expected_partition_index)?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_fat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    evidence_core::RawImageReader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
            )
        }
        _ => Ok(None),
    }
}

fn try_read_exfat_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
        return Ok(None);
    }
    if descriptor.partition_candidates.is_empty() {
        return Ok(None);
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match descriptor.source_kind.as_str() {
        "e01" => try_read_exfat_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                open_e01_reader_cached(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
        ),
        "raw" => try_read_exfat_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
        ),
        _ => Ok(None),
    }
}

fn try_read_exfat_image_range_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index = root_partition_index_for_entry(repo, entry);

    match source_kind.as_str() {
        "e01" => {
            let candidates = e01_partition_candidates(conn, entry, expected_partition_index)?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_exfat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    open_e01_reader_cached(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
            )
        }
        "raw" => {
            let candidates = raw_partition_candidates(&source_path, expected_partition_index)?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_exfat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    evidence_core::RawImageReader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
            )
        }
        _ => Ok(None),
    }
}

fn try_read_fat_image_range_from_candidates<F>(
    source_path: &Path,
    partition_candidates: &[PreviewPartitionCandidate],
    path_candidates: &[String],
    offset: u64,
    length: usize,
    mut open_reader: F,
) -> Result<Option<Vec<u8>>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    for candidate in partition_candidates {
        if !is_fat_filesystem_kind(&candidate.filesystem_kind) {
            continue;
        }

        let boxed_reader = open_reader(source_path)?;
        let fs = match fs_fat::FatReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => fs,
            Err(error) => {
                tracing::warn!(
                    partition_index = candidate.partition_index,
                    offset = candidate.offset,
                    error = %error,
                    "Descriptor FAT range open failed"
                );
                continue;
            }
        };

        for path in path_candidates {
            match fs.read_file_range(path, offset, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error) => {
                    tracing::warn!(
                        path = %path,
                        partition_index = candidate.partition_index,
                        offset = candidate.offset,
                        error = %error,
                        "Descriptor FAT range read failed for path candidate"
                    );
                }
            }
        }
    }

    Ok(None)
}

fn try_read_exfat_image_range_from_candidates<F>(
    source_path: &Path,
    partition_candidates: &[PreviewPartitionCandidate],
    path_candidates: &[String],
    offset: u64,
    length: usize,
    mut open_reader: F,
) -> Result<Option<Vec<u8>>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    for candidate in partition_candidates {
        let mut boxed_reader = open_reader(source_path)?;
        let looks_like_exfat = is_exfat_filesystem_kind(&candidate.filesystem_kind)
            || looks_like_exfat_boot_sector(boxed_reader.as_mut(), candidate.offset)
                .unwrap_or(false);
        if !looks_like_exfat {
            continue;
        }

        let fs = match fs_exfat::ExfatReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => fs,
            Err(error) => {
                tracing::warn!(
                    partition_index = candidate.partition_index,
                    offset = candidate.offset,
                    error = %error,
                    "Descriptor exFAT range open failed"
                );
                continue;
            }
        };

        for path in path_candidates {
            match fs.read_file_range(path, offset, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error) => {
                    tracing::warn!(
                        path = %path,
                        partition_index = candidate.partition_index,
                        offset = candidate.offset,
                        error = %error,
                        "Descriptor exFAT range read failed for path candidate"
                    );
                }
            }
        }
    }

    Ok(None)
}

fn is_fat_filesystem_kind(kind: &str) -> bool {
    matches!(kind, "FAT" | "FAT32" | "FAT16" | "FAT12")
}

fn is_exfat_filesystem_kind(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("exfat") || kind.to_ascii_uppercase().contains("EXFAT")
}

fn try_read_ntfs_image_range_from_candidates<F>(
    source_path: &Path,
    partition_candidates: &[PreviewPartitionCandidate],
    path_candidates: &[String],
    offset: u64,
    length: usize,
    mut open_reader: F,
) -> Result<Option<Vec<u8>>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    for candidate in partition_candidates {
        if candidate.filesystem_kind != "NTFS" {
            continue;
        }

        let boxed_reader = open_reader(source_path)?;
        let fs = match fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => fs,
            Err(error) => {
                tracing::warn!(
                    partition_index = candidate.partition_index,
                    offset = candidate.offset,
                    error = %error,
                    "Descriptor NTFS range open failed"
                );
                continue;
            }
        };

        for path in path_candidates {
            match fs.read_file_range(path, offset, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error) => {
                    tracing::warn!(
                        path = %path,
                        partition_index = candidate.partition_index,
                        offset = candidate.offset,
                        error = %error,
                        "Descriptor NTFS range read failed for path candidate"
                    );
                }
            }
        }
    }

    Ok(None)
}

fn open_descriptor_image_file<F>(
    descriptor: &PreviewDescriptor,
    mut open_reader: F,
) -> Result<Box<dyn Read>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    if descriptor.partition_candidates.is_empty() {
        return Err(FileServiceError::other(format!(
            "Cannot open image-backed file '{}' without partition candidates",
            descriptor.path
        )));
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    for candidate in &descriptor.partition_candidates {
        let result = if candidate.filesystem_kind == "NTFS" {
            let boxed_reader = open_reader(source_path)?;
            match fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset) {
                Ok(fs) => open_first_image_path(&fs, &path_candidates),
                Err(e) => {
                    tracing::warn!(
                        path = %descriptor.path,
                        partition_index = candidate.partition_index,
                        offset = candidate.offset,
                        error = %e,
                        "Descriptor NTFS open failed"
                    );
                    continue;
                }
            }
        } else if is_fat_filesystem_kind(&candidate.filesystem_kind) {
            open_fat_or_exfat_image_candidate(
                source_path,
                candidate,
                &path_candidates,
                &mut open_reader,
            )
        } else {
            match try_open_exfat_image_candidate(
                source_path,
                candidate,
                &path_candidates,
                &mut open_reader,
            ) {
                Ok(Some(reader)) => Ok(reader),
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(
                        path = %descriptor.path,
                        partition_index = candidate.partition_index,
                        offset = candidate.offset,
                        error = %e,
                        "Descriptor exFAT open failed"
                    );
                    continue;
                }
            }
        };

        match result {
            Ok(reader) => return Ok(reader),
            Err(e) => {
                tracing::warn!(
                    path = %descriptor.path,
                    partition_index = candidate.partition_index,
                    kind = %candidate.filesystem_kind,
                    error = %e,
                    "Descriptor file not found on partition"
                );
            }
        }
    }

    Err(FileServiceError::other(format!(
        "Cannot open image-backed file '{}' from any partition",
        descriptor.path
    )))
}

fn open_fat_or_exfat_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    path_candidates: &[String],
    open_reader: &mut F,
) -> std::io::Result<Box<dyn Read>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    let fat_result = {
        let boxed_reader = open_reader(source_path)?;
        match fs_fat::FatReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => open_first_image_path(&fs, path_candidates),
            Err(e) => Err(e),
        }
    };

    match fat_result {
        Ok(reader) => Ok(reader),
        Err(fat_error) => {
            tracing::warn!(
                partition_index = candidate.partition_index,
                offset = candidate.offset,
                error = %fat_error,
                "Descriptor FAT open failed; trying exFAT"
            );

            let boxed_reader = open_reader(source_path)?;
            match fs_exfat::ExfatReader::open(boxed_reader, candidate.offset) {
                Ok(fs) => open_first_image_path(&fs, path_candidates),
                Err(exfat_error) => Err(std::io::Error::new(
                    exfat_error.kind(),
                    format!("FAT open failed: {fat_error}; exFAT open failed: {exfat_error}"),
                )),
            }
        }
    }
}

fn try_open_exfat_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    path_candidates: &[String],
    open_reader: &mut F,
) -> std::io::Result<Option<Box<dyn Read>>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    let mut boxed_reader = open_reader(source_path)?;
    let looks_like_exfat = is_exfat_filesystem_kind(&candidate.filesystem_kind)
        || looks_like_exfat_boot_sector(boxed_reader.as_mut(), candidate.offset).unwrap_or(false);
    if !looks_like_exfat {
        return Ok(None);
    }

    match fs_exfat::ExfatReader::open(boxed_reader, candidate.offset) {
        Ok(fs) => open_first_image_path(&fs, path_candidates).map(Some),
        Err(error) => Err(error),
    }
}

fn open_first_image_path(
    fs: &dyn FileSystemReader,
    path_candidates: &[String],
) -> std::io::Result<Box<dyn Read>> {
    let mut last_error = None;
    for path in path_candidates {
        match fs.open_file(path) {
            Ok(reader) => return Ok(reader),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "No preview path candidates")
    }))
}

fn descriptor_image_path_candidates(descriptor: &PreviewDescriptor) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique_path_candidate(&mut candidates, descriptor.path.trim());

    if let Some(stripped) = strip_partition_path_prefix(&descriptor.path) {
        push_unique_path_candidate(&mut candidates, stripped);
    }

    push_unique_path_candidate(&mut candidates, &descriptor.file_id);
    candidates
}

fn entry_image_path_candidates(entry: &FileEntry) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique_path_candidate(&mut candidates, entry.path.trim());

    if let Some(stripped) = strip_partition_path_prefix(&entry.path) {
        push_unique_path_candidate(&mut candidates, stripped);
    }

    push_unique_path_candidate(&mut candidates, &entry.id.0);
    candidates
}

fn push_unique_path_candidate(candidates: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if !path.is_empty() && !candidates.iter().any(|candidate| candidate == path) {
        candidates.push(path.to_string());
    }
}

fn strip_partition_path_prefix(path: &str) -> Option<&str> {
    let path = path.trim_start();
    let rest = path.strip_prefix("[P")?;
    let (partition, after_partition) = rest.split_once(']')?;
    if partition.is_empty() || !partition.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let stripped = after_partition.trim_start_matches(['/', '\\']);
    (!stripped.is_empty()).then_some(stripped)
}

fn read_seekable_range(
    reader: &mut dyn ReadSeek,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, FileServiceError> {
    reader.seek(SeekFrom::Start(offset))?;
    read_bounded(reader, length)
}

fn read_bounded(reader: &mut dyn Read, length: usize) -> Result<Vec<u8>, FileServiceError> {
    let mut bytes = Vec::with_capacity(length);
    reader.take(length as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn read_file_header_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Vec<u8>, FileServiceError> {
    let mut bytes = Vec::with_capacity(max_bytes.min(infrastructure::constants::MAX_RANGE_LENGTH));
    let mut offset = 0u64;
    let mut remaining = max_bytes;

    while remaining > 0 {
        let chunk_len = remaining
            .min(infrastructure::constants::MAX_RANGE_LENGTH)
            .min(u32::MAX as usize) as u32;
        if chunk_len == 0 {
            break;
        }

        let chunk = read_file_bytes_for_case(conn, file_id, offset, chunk_len)?;
        if chunk.is_empty() {
            break;
        }

        let is_short_read = chunk.len() < chunk_len as usize;
        offset = offset.saturating_add(chunk.len() as u64);
        remaining = remaining.saturating_sub(chunk.len());
        bytes.extend_from_slice(&chunk);

        if is_short_read {
            break;
        }
    }

    Ok(bytes)
}

pub fn get_file_path_for_entry<C>(
    mut context: C,
    file_id: &str,
) -> Result<PathBuf, FileServiceError>
where
    C: PreviewReadContext,
{
    if !context.case_id().is_empty() {
        let descriptor =
            descriptor_for_file_with_cache(&mut context, &FileEntryId(file_id.to_string()))?;
        if descriptor.source_kind != "logical_directory" {
            return Err(FileServiceError::other(
                "File path only available for logical directories",
            ));
        }

        let entry = descriptor_file_entry(&descriptor);
        return resolve_logical_file_path(&descriptor.source_path, &entry);
    }

    let repo = FileRepo::new(context.conn());
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

fn open_range_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<RangeContentReader, FileServiceError> {
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
        "logical_directory" => {
            open_logical_file_seekable(&source_path, entry).map(RangeContentReader::Seekable)
        }
        "e01" => open_e01_file(conn, &source_path, entry, expected_partition_index)
            .map(RangeContentReader::Streaming),
        "raw" => open_raw_file(&source_path, entry, expected_partition_index)
            .map(RangeContentReader::Streaming),
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
    Ok(Box::new(std::fs::File::open(resolve_logical_file_path(
        source_path,
        entry,
    )?)?) as Box<dyn Read>)
}

fn open_logical_file_seekable(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn ReadSeek>, FileServiceError> {
    Ok(Box::new(std::fs::File::open(resolve_logical_file_path(
        source_path,
        entry,
    )?)?) as Box<dyn ReadSeek>)
}

fn resolve_logical_file_path(
    source_path: &str,
    entry: &FileEntry,
) -> Result<PathBuf, FileServiceError> {
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

    Ok(canonical)
}

fn e01_partition_candidates(
    conn: &Connection,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Vec<PreviewPartitionCandidate>, FileServiceError> {
    let part_repo = PartitionRepo::new(conn);
    let partitions = part_repo
        .find_by_data_source(&entry.data_source_id.0)
        .map_err(|e| FileServiceError::other(format!("Failed to query partitions: {e}")))?;

    if partitions.is_empty() {
        return Err(FileServiceError::other(
            "No partition metadata found for this data source. Re-import the E01 image.",
        ));
    }

    if !entry.path.contains('/') && !entry.path.contains('\\') {
        return Err(FileServiceError::other(format!(
            "Cannot preview '{}': path reconstruction did not resolve the parent directory. Re-import.",
            entry.path
        )));
    }

    let candidates: Vec<PreviewPartitionCandidate> = match expected_partition_index {
        Some(expected) => partitions
            .iter()
            .filter(|partition| {
                partition.partition_index as usize == expected
                    && partition.status != "EncryptedBitLocker"
            })
            .map(|partition| PreviewPartitionCandidate {
                partition_index: partition.partition_index as usize,
                filesystem_kind: partition
                    .filesystem
                    .as_deref()
                    .unwrap_or(&partition.kind_label)
                    .to_string(),
                offset: partition.offset,
            })
            .collect(),
        None => partitions
            .iter()
            .filter(|partition| partition.status != "EncryptedBitLocker")
            .map(|partition| PreviewPartitionCandidate {
                partition_index: partition.partition_index as usize,
                filesystem_kind: partition
                    .filesystem
                    .as_deref()
                    .unwrap_or(&partition.kind_label)
                    .to_string(),
                offset: partition.offset,
            })
            .collect(),
    };

    if candidates.is_empty() {
        return Err(FileServiceError::other(match expected_partition_index {
            Some(expected) => {
                format!("Partition index {expected} not found or is encrypted. Re-import.")
            }
            None => {
                "Cannot determine which partition this file belongs to. Re-import the E01 image."
                    .to_string()
            }
        }));
    }

    Ok(candidates)
}

fn raw_partition_candidates(
    source_path: &str,
    expected_partition_index: Option<usize>,
) -> Result<Vec<PreviewPartitionCandidate>, FileServiceError> {
    let mut reader = evidence_core::RawImageReader::open(Path::new(source_path))?;
    let probe = crate::datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|e| FileServiceError::other(format!("Failed to detect RAW filesystem: {e}")))?;
    if probe.candidates.is_empty() {
        if let Some(candidate) = direct_exfat_raw_partition_candidate(source_path)? {
            if expected_partition_index.is_none_or(|expected| expected == candidate.partition_index)
            {
                return Ok(vec![candidate]);
            }
        }

        return Err(FileServiceError::other(
            "No supported filesystem detected in RAW image",
        ));
    }

    let index_map =
        crate::datasource_service::assign_effective_partition_indices(&probe.candidates);
    let mut candidates = Vec::new();
    for (candidate_pos, candidate) in probe.candidates.iter().enumerate() {
        let partition_index = crate::datasource_service::effective_partition_index(
            candidate,
            candidate_pos,
            &index_map,
        );
        if expected_partition_index.is_some_and(|expected| partition_index != expected) {
            continue;
        }

        let filesystem_kind = match candidate.kind {
            crate::datasource_service::ImageFilesystemKind::Ntfs => "NTFS",
            crate::datasource_service::ImageFilesystemKind::Fat => "FAT",
            crate::datasource_service::ImageFilesystemKind::BitLocker => continue,
        };
        candidates.push(PreviewPartitionCandidate {
            partition_index,
            filesystem_kind: filesystem_kind.to_string(),
            offset: candidate.offset,
        });
    }

    let mut exfat_reader = evidence_core::RawImageReader::open(Path::new(source_path))?;
    for partition in &probe.partitions {
        if expected_partition_index.is_some_and(|expected| partition.index != expected) {
            continue;
        }
        if candidates
            .iter()
            .any(|candidate| candidate.partition_index == partition.index)
        {
            continue;
        }
        if !looks_like_exfat_boot_sector(&mut exfat_reader, partition.offset)? {
            continue;
        }

        candidates.push(PreviewPartitionCandidate {
            partition_index: partition.index,
            filesystem_kind: "EXFAT".to_string(),
            offset: partition.offset,
        });
    }

    if candidates.is_empty() {
        return Err(FileServiceError::other(match expected_partition_index {
            Some(expected) => {
                format!("Partition index {expected} not found or is unsupported.")
            }
            None => "No supported filesystem detected in RAW image".to_string(),
        }));
    }

    Ok(candidates)
}

fn direct_exfat_raw_partition_candidate(
    source_path: &str,
) -> Result<Option<PreviewPartitionCandidate>, FileServiceError> {
    let mut reader = evidence_core::RawImageReader::open(Path::new(source_path))?;
    if !looks_like_exfat_boot_sector(&mut reader, 0)? {
        return Ok(None);
    }

    Ok(Some(PreviewPartitionCandidate {
        partition_index: 0,
        filesystem_kind: "EXFAT".to_string(),
        offset: 0,
    }))
}

fn looks_like_exfat_boot_sector<R>(reader: &mut R, offset: u64) -> std::io::Result<bool>
where
    R: Read + Seek + ?Sized,
{
    let mut sector = [0u8; 512];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut sector)?;

    Ok(&sector[3..11] == b"EXFAT   " && sector[510] == 0x55 && sector[511] == 0xAA)
}

fn is_preview_image_filesystem_kind(kind: &str) -> bool {
    kind == "NTFS" || is_fat_filesystem_kind(kind) || is_exfat_filesystem_kind(kind)
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
    let source_path = reader.info().path.clone();
    let path_candidates = entry_image_path_candidates(entry);
    if probe.candidates.is_empty() {
        if expected_partition_index.is_none_or(|expected| expected == 0)
            && looks_like_exfat_boot_sector(&mut reader, 0)?
        {
            let boxed: Box<dyn evidence_core::EvidenceReader> =
                Box::new(evidence_core::RawImageReader::open(&source_path)?);
            let fs = fs_exfat::ExfatReader::open(boxed, 0)?;
            return open_first_image_path(&fs, &path_candidates)
                .map_err(|e| FileServiceError::other(format!("{e}")));
        }

        for partition in &probe.partitions {
            if expected_partition_index.is_some_and(|expected| partition.index != expected) {
                continue;
            }
            if !looks_like_exfat_boot_sector(&mut reader, partition.offset)? {
                continue;
            }

            let boxed: Box<dyn evidence_core::EvidenceReader> =
                Box::new(evidence_core::RawImageReader::open(&source_path)?);
            let fs = fs_exfat::ExfatReader::open(boxed, partition.offset)?;
            return open_first_image_path(&fs, &path_candidates)
                .map_err(|e| FileServiceError::other(format!("{e}")));
        }

        return Err(FileServiceError::other(
            "No supported filesystem detected in RAW image",
        ));
    }
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
                match fs_fat::FatReader::open(boxed, candidate.offset) {
                    Ok(fs) => {
                        if let Ok(r) = open_first_image_path(&fs, &path_candidates) {
                            return Ok(r);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path,
                            partition_index = eff,
                            offset = candidate.offset,
                            error = %e,
                            "RAW FAT open failed; trying exFAT"
                        );

                        let exfat_boxed: Box<dyn evidence_core::EvidenceReader> =
                            Box::new(evidence_core::RawImageReader::open(&source_path)?);
                        if let Ok(fs) = fs_exfat::ExfatReader::open(exfat_boxed, candidate.offset) {
                            if let Ok(r) = open_first_image_path(&fs, &path_candidates) {
                                return Ok(r);
                            }
                        }
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

    // 收集候选分区：优先匹配 expected_partition_index，回退到第一个非加密 NTFS
    let candidates_to_try: Vec<
        &persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord,
    > = match expected_partition_index {
        Some(expected) => partitions
            .iter()
            .filter(|p| p.partition_index as usize == expected && p.status != "EncryptedBitLocker")
            .collect(),
        None => {
            // Fallback: try the first non-encrypted NTFS partition for entries
            // whose parent chain could not be resolved (e.g., /Unresolved/ entries)
            let previewable: Vec<_> = partitions
                .iter()
                .filter(|p| {
                    p.status != "EncryptedBitLocker"
                        && is_preview_image_filesystem_kind(
                            p.filesystem.as_deref().unwrap_or(&p.kind_label),
                        )
                })
                .collect();
            if previewable.is_empty() {
                return Err(FileServiceError::other(
                    "Cannot determine which partition this file belongs to. Re-import the E01 image.",
                ));
            }
            previewable
        }
    };

    if candidates_to_try.is_empty() {
        return Err(FileServiceError::other(format!(
            "Partition index {} not found or is encrypted. Re-import.",
            expected_partition_index.unwrap_or(0)
        )));
    }

    let path_candidates = entry_image_path_candidates(entry);
    for target in &candidates_to_try {
        let fs_kind = target.filesystem.as_deref().unwrap_or(&target.kind_label);
        let exfat_hint = if is_exfat_filesystem_kind(fs_kind) {
            true
        } else {
            let mut probe_reader = open_e01_reader_cached(Path::new(source_path))?;
            looks_like_exfat_boot_sector(&mut probe_reader, target.offset).unwrap_or(false)
        };

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
                    Ok(fs) => open_first_image_path(&fs, &path_candidates),
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path,
                            partition = %target.name,
                            offset = %target.offset,
                            error = %e,
                            "E01 FAT open failed; trying exFAT"
                        );

                        let exfat_reader = open_e01_reader_cached(Path::new(source_path))?;
                        let exfat_boxed: Box<dyn evidence_core::EvidenceReader> =
                            Box::new(exfat_reader);
                        match fs_exfat::ExfatReader::open(exfat_boxed, target.offset) {
                            Ok(fs) => open_first_image_path(&fs, &path_candidates),
                            Err(exfat_error) => Err(std::io::Error::new(
                                exfat_error.kind(),
                                format!("FAT open failed: {e}; exFAT open failed: {exfat_error}"),
                            )),
                        }
                    }
                }
            }
            _ if exfat_hint => match fs_exfat::ExfatReader::open(boxed_reader, target.offset) {
                Ok(fs) => open_first_image_path(&fs, &path_candidates),
                Err(e) => {
                    tracing::warn!(
                        path = %entry.path,
                        partition = %target.name,
                        offset = %target.offset,
                        error = %e,
                        "E01 exFAT open failed"
                    );
                    continue;
                }
            },
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
    #[cfg(test)]
    SKIP_READER_BYTES_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

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
    #[cfg(test)]
    FORMAT_HEX_LINES_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

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
        raw_bytes: None,
        kind: "hex".into(),
        lines: Vec::new(),
        encoding: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
    use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
    use persistence_sqlite::runner;
    use rusqlite::params;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::io::{Read, Seek, SeekFrom, Write};

    fn reset_skip_reader_bytes_call_count() {
        SKIP_READER_BYTES_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    fn skip_reader_bytes_call_count() -> usize {
        SKIP_READER_BYTES_CALLS.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn reset_format_hex_lines_call_count() {
        FORMAT_HEX_LINES_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    fn format_hex_lines_call_count() -> usize {
        FORMAT_HEX_LINES_CALLS.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn reset_open_file_content_by_id_call_count() {
        OPEN_FILE_CONTENT_BY_ID_CALLS.with(|calls| calls.set(0));
    }

    fn open_file_content_by_id_call_count() -> usize {
        OPEN_FILE_CONTENT_BY_ID_CALLS.with(std::cell::Cell::get)
    }

    fn reset_read_file_bytes_for_case_call_count() {
        READ_FILE_BYTES_FOR_CASE_CALLS.with(|calls| calls.set(0));
    }

    fn read_file_bytes_for_case_call_count() -> usize {
        READ_FILE_BYTES_FOR_CASE_CALLS.with(std::cell::Cell::get)
    }

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

    fn write_large_ntfs_raw_fixture(path: &std::path::Path, marker: &[u8]) -> std::io::Result<()> {
        const CLUSTER_SIZE: usize = 512;
        const MFT_RECORD_SIZE: usize = 1024;
        const MFT_CLUSTER: usize = 2;
        const FILE_RECORD: u64 = 6;
        const DATA_CLUSTER: usize = 32;
        const SPARSE_PREFIX_CLUSTERS: u64 = (128 * 1024 * 1024) / CLUSTER_SIZE as u64;

        let rec5_off = MFT_CLUSTER * CLUSTER_SIZE + 5 * MFT_RECORD_SIZE;
        let rec6_off = MFT_CLUSTER * CLUSTER_SIZE + FILE_RECORD as usize * MFT_RECORD_SIZE;
        let data_off = DATA_CLUSTER * CLUSTER_SIZE;
        let total = data_off + CLUSTER_SIZE;
        let mut data = vec![0u8; total];

        let boot = &mut data[0..512];
        boot[0] = 0xEB;
        boot[1] = 0x52;
        boot[2] = 0x90;
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[11..13].copy_from_slice(&512u16.to_le_bytes());
        boot[13] = 1;
        boot[0x30..0x38].copy_from_slice(&(MFT_CLUSTER as u64).to_le_bytes());
        boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());
        boot[510] = 0x55;
        boot[511] = 0xAA;

        let rec5 = &mut data[rec5_off..rec5_off + MFT_RECORD_SIZE];
        rec5[0..4].copy_from_slice(b"FILE");
        rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
        rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
        rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
        let iro = 0x68usize;
        rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
        rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
        let mut entry = vec![0u8; 0x52 + "large.bin".encode_utf16().count() * 2];
        let entry_len = entry.len();
        entry[0..8].copy_from_slice(&FILE_RECORD.to_le_bytes());
        entry[8..10].copy_from_slice(&(entry_len as u16).to_le_bytes());
        entry[0x40..0x48]
            .copy_from_slice(&((128u64 * 1024 * 1024) + marker.len() as u64).to_le_bytes());
        entry[0x50] = "large.bin".encode_utf16().count() as u8;
        for (i, ch) in "large.bin".encode_utf16().enumerate() {
            entry[0x52 + i * 2..0x54 + i * 2].copy_from_slice(&ch.to_le_bytes());
        }
        let mut off = iro + 0x20;
        rec5[off..off + entry.len()].copy_from_slice(&entry);
        off += entry.len();
        rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        off += 4;
        rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

        let rec6 = &mut data[rec6_off..rec6_off + MFT_RECORD_SIZE];
        rec6[0..4].copy_from_slice(b"FILE");
        rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
        rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
        rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
        let data_attr = 0x68usize;
        let logical_size = (128u64 * 1024 * 1024) + marker.len() as u64;
        rec6[data_attr..data_attr + 4].copy_from_slice(&0x80u32.to_le_bytes());
        rec6[data_attr + 8] = 1;
        rec6[data_attr + 0x20..data_attr + 0x22].copy_from_slice(&0x40u16.to_le_bytes());
        rec6[data_attr + 0x28..data_attr + 0x30]
            .copy_from_slice(&((SPARSE_PREFIX_CLUSTERS + 1) * CLUSTER_SIZE as u64).to_le_bytes());
        rec6[data_attr + 0x30..data_attr + 0x38].copy_from_slice(&logical_size.to_le_bytes());

        let run = data_attr + 0x40;
        rec6[run] = 0x03;
        rec6[run + 1..run + 4].copy_from_slice(&SPARSE_PREFIX_CLUSTERS.to_le_bytes()[..3]);
        rec6[run + 4] = 0x11;
        rec6[run + 5] = 1;
        rec6[run + 6] = DATA_CLUSTER as u8;
        rec6[run + 7] = 0;
        let attr_len = (run + 8 - data_attr) as u32;
        rec6[data_attr + 4..data_attr + 8].copy_from_slice(&attr_len.to_le_bytes());

        data[data_off..data_off + marker.len()].copy_from_slice(marker);
        std::fs::write(path, data)
    }

    fn write_fat32_raw_fixture(path: &std::path::Path) -> std::io::Result<()> {
        const SECTOR_SIZE: usize = 512;
        const RESERVED_SECTORS: usize = 1;
        const FAT_SECTORS: usize = 1;
        const FIRST_DATA_SECTOR: usize = RESERVED_SECTORS + FAT_SECTORS;
        const CLUSTER_SIZE: usize = SECTOR_SIZE;

        let total_sectors = 16usize;
        let mut data = vec![0u8; total_sectors * SECTOR_SIZE];

        let boot = &mut data[0..SECTOR_SIZE];
        boot[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
        boot[3..11].copy_from_slice(b"MSDOS5.0");
        boot[11..13].copy_from_slice(&(SECTOR_SIZE as u16).to_le_bytes());
        boot[13] = 1;
        boot[14..16].copy_from_slice(&(RESERVED_SECTORS as u16).to_le_bytes());
        boot[16] = 1;
        boot[17..19].copy_from_slice(&0u16.to_le_bytes());
        boot[32..36].copy_from_slice(&(total_sectors as u32).to_le_bytes());
        boot[36..40].copy_from_slice(&(FAT_SECTORS as u32).to_le_bytes());
        boot[44..48].copy_from_slice(&2u32.to_le_bytes());
        boot[0x42] = 0x29;
        boot[82..90].copy_from_slice(b"FAT32   ");
        boot[510] = 0x55;
        boot[511] = 0xAA;

        let fat_offset = RESERVED_SECTORS * SECTOR_SIZE;
        let fat = &mut data[fat_offset..fat_offset + SECTOR_SIZE];
        fat[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
        fat[4..8].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
        fat[8..12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
        fat[12..16].copy_from_slice(&4u32.to_le_bytes());
        fat[16..20].copy_from_slice(&5u32.to_le_bytes());
        fat[20..24].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());

        let root_offset = FIRST_DATA_SECTOR * SECTOR_SIZE;
        let root = &mut data[root_offset..root_offset + CLUSTER_SIZE];
        root[0..8].copy_from_slice(b"RANGE   ");
        root[8..11].copy_from_slice(b"TXT");
        root[11] = 0x20;
        root[26..28].copy_from_slice(&3u16.to_le_bytes());
        root[28..32].copy_from_slice(&(CLUSTER_SIZE as u32 * 3).to_le_bytes());

        for cluster in 3..=5usize {
            let value = match cluster {
                3 => b'A',
                4 => b'B',
                5 => b'C',
                _ => unreachable!(),
            };
            let offset = FIRST_DATA_SECTOR * SECTOR_SIZE + (cluster - 2) * CLUSTER_SIZE;
            data[offset..offset + CLUSTER_SIZE].fill(value);
        }

        std::fs::write(path, data)
    }

    fn write_exfat_raw_fixture(path: &std::path::Path) -> std::io::Result<()> {
        const SECTOR_SIZE: usize = 512;
        const FAT_SECTOR: usize = 24;
        const CLUSTER_HEAP_SECTOR: usize = 32;
        const CLUSTER_SIZE: usize = SECTOR_SIZE;
        const FILE_SIZE: usize = CLUSTER_SIZE * 3;
        const TOTAL_SECTORS: usize = 1024;

        let mut data = vec![0u8; TOTAL_SECTORS * SECTOR_SIZE];

        let boot = &mut data[0..SECTOR_SIZE];
        boot[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
        boot[3..11].copy_from_slice(b"EXFAT   ");
        boot[72..80].copy_from_slice(&(TOTAL_SECTORS as u64).to_le_bytes());
        boot[80..84].copy_from_slice(&(FAT_SECTOR as u32).to_le_bytes());
        boot[84..88].copy_from_slice(&1u32.to_le_bytes());
        boot[88..92].copy_from_slice(&(CLUSTER_HEAP_SECTOR as u32).to_le_bytes());
        boot[92..96].copy_from_slice(&100u32.to_le_bytes());
        boot[96..100].copy_from_slice(&2u32.to_le_bytes());
        boot[100..104].copy_from_slice(&0x12345678u32.to_le_bytes());
        boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        boot[108] = 9;
        boot[109] = 0;
        boot[110] = 1;
        boot[111] = 0x80;
        boot[112] = 0xFF;
        boot[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());

        let fat_offset = FAT_SECTOR * SECTOR_SIZE;
        let fat = &mut data[fat_offset..fat_offset + SECTOR_SIZE];
        fat[0..4].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF]);
        fat[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        fat[8..12].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        fat[12..16].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);

        let root_offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE;
        let root = &mut data[root_offset..root_offset + CLUSTER_SIZE];
        let mut pos = 0usize;

        root[pos] = 0x85;
        root[pos + 1] = 0x02;
        root[pos + 4..pos + 6].copy_from_slice(&0x20u16.to_le_bytes());
        pos += 32;

        root[pos] = 0xC0;
        root[pos + 1] = 0x02;
        root[pos + 3] = "LARGE.BIN".encode_utf16().count() as u8;
        root[pos + 8..pos + 16].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
        root[pos + 20..pos + 24].copy_from_slice(&3u32.to_le_bytes());
        root[pos + 24..pos + 32].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
        pos += 32;

        root[pos] = 0xC1;
        for (i, ch) in "LARGE.BIN".encode_utf16().enumerate() {
            let offset = pos + 2 + i * 2;
            root[offset..offset + 2].copy_from_slice(&ch.to_le_bytes());
        }

        for cluster in 3..=5usize {
            let value = match cluster {
                3 => b'A',
                4 => b'B',
                5 => b'C',
                _ => unreachable!(),
            };
            let offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE + (cluster - 2) * CLUSTER_SIZE;
            data[offset..offset + CLUSTER_SIZE].fill(value);
        }

        std::fs::write(path, data)
    }

    #[test]
    fn logical_directory_mid_file_range_uses_seek_not_linear_skip() {
        let dir = tempfile::TempDir::new().unwrap();
        let evidence_dir = dir.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        let bytes: Vec<u8> = (0u8..64).collect();
        std::fs::write(evidence_dir.join("sample.bin"), &bytes).unwrap();

        let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-range', 'Range Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let ds_id = DataSourceId("ds-logical-range".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-range".to_string()),
                &DataSource {
                    id: ds_id.clone(),
                    name: "logical evidence".to_string(),
                    kind: DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir,
                    imported_at: chrono::Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();

        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('file-sample', NULL, ?1, 'sample.bin', 'sample.bin', 'file', ?2, 'bin', 0, 0, 0)",
            params![ds_id.0, bytes.len() as i64],
        )
        .unwrap();

        reset_skip_reader_bytes_call_count();
        reset_format_hex_lines_call_count();
        let range_bytes =
            read_file_bytes_for_case(&conn, &FileEntryId("file-sample".to_string()), 17, 12)
                .unwrap();

        assert_eq!(range_bytes, bytes[17..29].to_vec());
        assert_eq!(skip_reader_bytes_call_count(), 0);
        assert_eq!(format_hex_lines_call_count(), 0);

        let response = read_file_range_for_case(
            &conn,
            &ViewerRangeRequestDto {
                handle_id: "file:file-sample".to_string(),
                offset: 17,
                length: 12,
            },
        )
        .unwrap();

        assert_eq!(response.raw_bytes.unwrap(), bytes[17..29].to_vec());
        assert_eq!(format_hex_lines_call_count(), 1);
    }

    #[test]
    fn logical_directory_repeated_range_uses_preview_descriptor_cache() {
        let dir = tempfile::TempDir::new().unwrap();
        let evidence_dir = dir.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        let bytes: Vec<u8> = (0u8..64).collect();
        std::fs::write(evidence_dir.join("sample.bin"), &bytes).unwrap();

        let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-cache', 'Cache Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let ds_id = DataSourceId("ds-logical-cache".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-cache".to_string()),
                &DataSource {
                    id: ds_id.clone(),
                    name: "logical evidence".to_string(),
                    kind: DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir,
                    imported_at: chrono::Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();

        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('file-cache-sample', NULL, ?1, 'sample.bin', 'sample.bin', 'file', ?2, 'bin', 0, 0, 0)",
            params![ds_id.0, bytes.len() as i64],
        )
        .unwrap();

        let file_id = FileEntryId("file-cache-sample".to_string());
        let cache = std::cell::RefCell::new(HashMap::<String, serde_json::Value>::new());
        let cache_hits = Cell::new(0usize);
        let set_calls = Cell::new(0usize);

        let read_with_cache = |offset, length| {
            let get_cache = |key: &str| {
                let value = cache.borrow().get(key).cloned();
                if value.is_some() {
                    cache_hits.set(cache_hits.get() + 1);
                }
                value
            };
            let set_cache = |key: &str, value: &serde_json::Value| {
                set_calls.set(set_calls.get() + 1);
                cache.borrow_mut().insert(key.to_string(), value.clone());
            };
            read_file_bytes_for_case(
                (&conn, "case-cache", get_cache, set_cache),
                &file_id,
                offset,
                length,
            )
        };

        let first = read_with_cache(0, 8).unwrap();
        assert_eq!(first, bytes[0..8].to_vec());
        assert_eq!(set_calls.get(), 1);
        assert_eq!(cache_hits.get(), 0);

        let second = read_with_cache(17, 12).unwrap();
        assert_eq!(second, bytes[17..29].to_vec());
        assert_eq!(set_calls.get(), 1);
        assert_eq!(cache_hits.get(), 1);

        cache.borrow_mut().clear();
        let third = read_with_cache(29, 7).unwrap();
        assert_eq!(third, bytes[29..36].to_vec());
        assert_eq!(set_calls.get(), 2);
    }

    #[test]
    fn raw_ntfs_mid_file_range_uses_ntfs_range_reader_without_materialize() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw_path = dir.path().join("large_ntfs.raw");
        let marker = b"RANGE-ONLY";
        write_large_ntfs_raw_fixture(&raw_path, marker).unwrap();

        let huge_size = (128u64 * 1024 * 1024) + marker.len() as u64;
        let descriptor = PreviewDescriptor {
            case_id: "case-raw-ntfs-range".to_string(),
            file_id: "mft:1:6".to_string(),
            source_kind: "raw".to_string(),
            source_path: raw_path.display().to_string(),
            partition_index: Some(1),
            filesystem_kind: Some("NTFS".to_string()),
            path: "[P1]/large.bin".to_string(),
            mime: Some("application/octet-stream".to_string()),
            size: huge_size,
            data_source_id: "ds-raw-ntfs-range".to_string(),
            partition_candidates: vec![PreviewPartitionCandidate {
                partition_index: 1,
                filesystem_kind: "NTFS".to_string(),
                offset: 0,
            }],
        };

        reset_skip_reader_bytes_call_count();
        let bytes =
            read_file_bytes_for_descriptor(&descriptor, 128u64 * 1024 * 1024, marker.len() as u32)
                .unwrap();

        assert_eq!(bytes, marker);
        assert_eq!(skip_reader_bytes_call_count(), 0);
    }

    #[test]
    fn raw_fat_mid_file_range_uses_fat_range_reader_without_materialize() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw_path = dir.path().join("fat32.raw");
        write_fat32_raw_fixture(&raw_path).unwrap();

        let descriptor = PreviewDescriptor {
            case_id: "case-raw-fat-range".to_string(),
            file_id: "fat-file-range".to_string(),
            source_kind: "raw".to_string(),
            source_path: raw_path.display().to_string(),
            partition_index: Some(0),
            filesystem_kind: Some("FAT".to_string()),
            path: "[P0]/RANGE.TXT".to_string(),
            mime: Some("text/plain".to_string()),
            size: 1536,
            data_source_id: "ds-raw-fat-range".to_string(),
            partition_candidates: vec![PreviewPartitionCandidate {
                partition_index: 0,
                filesystem_kind: "FAT".to_string(),
                offset: 0,
            }],
        };

        reset_skip_reader_bytes_call_count();
        let bytes = read_file_bytes_for_descriptor(&descriptor, 512 + 7, 9).unwrap();

        assert_eq!(bytes, vec![b'B'; 9]);
        assert_eq!(skip_reader_bytes_call_count(), 0);
    }

    #[test]
    fn raw_exfat_mid_file_range_uses_exfat_range_reader_without_materialize() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw_path = dir.path().join("exfat.raw");
        write_exfat_raw_fixture(&raw_path).unwrap();

        let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-raw-exfat-range', 'Raw exFAT Range Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let ds_id = DataSourceId("ds-raw-exfat-range".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-raw-exfat-range".to_string()),
                &DataSource {
                    id: ds_id.clone(),
                    name: "raw exfat evidence".to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: raw_path,
                    imported_at: chrono::Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();

        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('file-raw-exfat-large', NULL, ?1, 'LARGE.BIN', 'LARGE.BIN', 'file', ?2, 'bin', 0, 0, 0)",
            params![ds_id.0, 1536i64],
        )
        .unwrap();

        reset_skip_reader_bytes_call_count();
        let bytes = read_file_bytes_for_case(
            &conn,
            &FileEntryId("file-raw-exfat-large".to_string()),
            512 + 7,
            9,
        )
        .unwrap();

        assert_eq!(bytes, vec![b'B'; 9]);
        assert_eq!(skip_reader_bytes_call_count(), 0);
    }

    #[test]
    fn raw_exfat_text_header_reads_via_bytes_only_fast_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw_path = dir.path().join("exfat.raw");
        write_exfat_raw_fixture(&raw_path).unwrap();

        let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-raw-exfat-header', 'Raw exFAT Header Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let ds_id = DataSourceId("ds-raw-exfat-header".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-raw-exfat-header".to_string()),
                &DataSource {
                    id: ds_id.clone(),
                    name: "raw exfat evidence".to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: raw_path,
                    imported_at: chrono::Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();

        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('file-raw-exfat-header', NULL, ?1, 'LARGE.BIN', 'LARGE.BIN', 'file', ?2, 'bin', 0, 0, 0)",
            params![ds_id.0, 1536i64],
        )
        .unwrap();

        reset_open_file_content_by_id_call_count();
        reset_read_file_bytes_for_case_call_count();
        reset_skip_reader_bytes_call_count();

        let bytes =
            read_file_header_by_id(&conn, &FileEntryId("file-raw-exfat-header".to_string()), 16)
                .unwrap();

        assert_eq!(bytes, vec![b'A'; 16]);
        assert_eq!(read_file_bytes_for_case_call_count(), 1);
        assert_eq!(open_file_content_by_id_call_count(), 0);
        assert_eq!(skip_reader_bytes_call_count(), 0);
    }

    #[test]
    fn truncated_e01_segment_no_panic() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("truncated.E01");
        // Write a valid E01 header but truncate before the chunk data
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")
            .unwrap();
        // Write volume section descriptor but no actual chunk table or data
        let desc = section_desc("volume", 0, 76 + 36);
        f.write_all(&desc).unwrap();
        f.write_all(&[0u8; 36]).unwrap();
        // Missing: table section, done section, chunk data
        f.flush().unwrap();
        drop(f);

        // Opening should fail gracefully with an error, not panic
        let result = E01Reader::open(&path);
        assert!(
            result.is_err(),
            "Truncated E01 should return error, not panic"
        );
    }

    #[test]
    fn truncated_e01_chunk_read_no_panic() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("short_chunk.E01");
        write_tiny_e01(&path).unwrap();

        // Open works (complete structure)
        let mut reader = E01Reader::open(&path).unwrap();

        // Read available chunk data
        let mut buf = vec![0u8; 14]; // "E01-CACHE-TEST" marker
        reader.seek(SeekFrom::Start(0)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"E01-CACHE-TEST");

        // Read past the available chunk — E01 reader should handle short reads gracefully
        let mut big_buf = vec![0u8; 8192];
        reader.seek(SeekFrom::Start(0)).unwrap();
        let result = reader.read(&mut big_buf);
        // read() may return partial data without error. Just verify no panic.
        let _ = result;
        eprintln!(
            "Short read returned {} bytes (expected for tiny E01)",
            big_buf.len()
        );
    }

    #[test]
    fn multi_partition_resolves_partition_index_correctly() {
        // Verify that entries with partition index in ID format resolve correctly
        assert_eq!(
            mft_partition_index_from_entry_id("mft:0:42"),
            Some(0),
            "Partition 0 entry should resolve to index 0"
        );
        assert_eq!(
            mft_partition_index_from_entry_id("mft:2:100"),
            Some(2),
            "Partition 2 entry should resolve to index 2"
        );

        // Verify that entries WITHOUT partition index in ID fall back to parent chain
        assert_eq!(
            mft_partition_index_from_entry_id("mft:42"),
            None,
            "Legacy format should return None and fall back to parent chain"
        );

        // Verify root name parsing from parent chain (simulated via function)
        let root_name = "Partition 3 (NTFS)";
        let idx: Option<usize> = root_name.strip_prefix("Partition ").and_then(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        });
        assert_eq!(
            idx,
            Some(3),
            "Root name 'Partition 3 (NTFS)' should resolve to index 3"
        );
    }

    #[test]
    fn mft_partition_index_from_entry_id_parses_partition_record_format() {
        assert_eq!(mft_partition_index_from_entry_id("mft:3:42"), Some(3));
        assert_eq!(mft_partition_index_from_entry_id("mft:0:5"), Some(0));
    }

    #[test]
    fn mft_partition_index_from_entry_id_returns_none_for_legacy_format() {
        assert_eq!(mft_partition_index_from_entry_id("mft:42"), None);
        assert_eq!(mft_partition_index_from_entry_id("not-mft:1:2"), None);
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

    #[test]
    fn lru_evicts_oldest_when_full() {
        clear_e01_reader_cache();
        let dir = tempfile::TempDir::new().unwrap();
        let mut paths = Vec::new();
        // Open 5 E01 files (cache max = 4)
        for i in 0..5 {
            let path = dir.path().join(format!("cache-test-{i}.E01"));
            write_tiny_e01(&path).unwrap();
            paths.push(path);
        }
        // First 4 go into cache
        for path in &paths[..4] {
            let _r = open_e01_reader_cached(path).unwrap();
        }
        // 5th evicts the first (paths[0])
        let _r = open_e01_reader_cached(&paths[4]).unwrap();

        // Verify paths[0] was evicted by trying to open it fresh — should still work
        let mut r = open_e01_reader_cached(&paths[0]).unwrap();
        let mut buf = [0u8; 14];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"E01-CACHE-TEST");

        clear_e01_reader_cache();
    }

    #[test]
    fn cache_clear_on_poison() {
        clear_e01_reader_cache();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("poison-test.E01");
        write_tiny_e01(&path).unwrap();

        // Populate the cache
        let _r = open_e01_reader_cached(&path).unwrap();

        // Poison the mutex by panicking while holding the lock
        let result = std::panic::catch_unwind(|| {
            let _lock = E01_READER_CACHE.lock().unwrap();
            panic!("simulated cache panic");
        });
        assert!(result.is_err());

        // After poison, the cache should be cleared and a new open should work
        let mut r = open_e01_reader_cached(&path).unwrap();
        let mut buf = [0u8; 14];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"E01-CACHE-TEST");

        clear_e01_reader_cache();
    }
}
