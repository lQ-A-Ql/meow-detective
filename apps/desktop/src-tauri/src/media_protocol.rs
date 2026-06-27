use crate::state::AppState;
use app_services::file_service;
use base64::Engine;
use chrono::Duration;
use runtime_cache::models::{namespaces, CacheEntry};
use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};
use tauri::http::{self, header, StatusCode};
use tauri::{AppHandle, Manager, Wry};

pub const EVIDENCE_MEDIA_SCHEME: &str = "evidence-media";
pub const MAX_MEDIA_PROTOCOL_READ_BYTES: u64 = transport::dto::MAX_VIEWER_RANGE_LENGTH as u64;
const MEDIA_HANDLE_TTL_MINUTES: i64 = 30;
const PREVIEW_DESCRIPTOR_CACHE_TTL_MINUTES: i64 = 30;

#[cfg(test)]
static MEDIA_PROTOCOL_BYTES_SERVICE_READ_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

macro_rules! with_preview_cache_context {
    ($state:expr, $conn:expr, $case_id:expr, |$context:ident| $body:expr) => {{
        let mut get_cached_preview_descriptor = |key: &str| -> Option<serde_json::Value> {
            let cache = $state.runtime_cache.lock().ok()?;
            let entry = cache.cache().get(key).ok().flatten()?;
            if entry.namespace != namespaces::PREVIEW_DESCRIPTORS
                || entry.case_id.as_deref() != Some($case_id)
            {
                return None;
            }
            Some(entry.value_json)
        };
        let mut set_cached_preview_descriptor = |key: &str, value: &serde_json::Value| {
            let Ok(cache) = $state.runtime_cache.lock() else {
                return;
            };
            let now = chrono::Utc::now();
            let entry = CacheEntry {
                cache_key: key.to_string(),
                namespace: namespaces::PREVIEW_DESCRIPTORS.to_string(),
                case_id: Some($case_id.to_string()),
                value_json: value.clone(),
                created_at: now,
                expires_at: Some(now + Duration::minutes(PREVIEW_DESCRIPTOR_CACHE_TTL_MINUTES)),
                last_accessed_at: now,
            };
            if let Err(error) = cache.cache().set(&entry) {
                tracing::warn!(cache_key = %key, error = %error, "Failed to cache preview descriptor");
            }
        };
        let $context = (
            $conn,
            $case_id,
            &mut get_cached_preview_descriptor,
            &mut set_cached_preview_descriptor,
        );
        $body
    }};
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRange {
    pub start: u64,
    pub end: u64,
    pub length: u64,
    pub status: StatusCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    EmptyFile,
    Invalid,
    Unsatisfiable,
}

impl RangeError {
    fn status(&self) -> StatusCode {
        match self {
            Self::EmptyFile | Self::Invalid | Self::Unsatisfiable => {
                StatusCode::RANGE_NOT_SATISFIABLE
            }
        }
    }
}

pub fn media_protocol_url(handle_id: &str) -> String {
    format!(
        "{EVIDENCE_MEDIA_SCHEME}://handle/{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(handle_id)
    )
}

pub fn create_scoped_media_handle(state: &AppState, file_id: &str) -> Result<String, String> {
    let case_id = state
        .active_case
        .lock()
        .map_err(|_| "media handle unavailable".to_string())?
        .as_ref()
        .map(|active| active.meta.id.0.clone())
        .ok_or_else(|| "media handle unavailable".to_string())?;

    let cache = state
        .runtime_cache
        .lock()
        .map_err(|_| "media handle unavailable".to_string())?;
    cache
        .handles()
        .create(
            &case_id,
            file_id,
            Duration::minutes(MEDIA_HANDLE_TTL_MINUTES),
        )
        .map_err(|_| "media handle unavailable".to_string())
}

pub fn resolve_scoped_media_handle(state: &AppState, handle_id: &str) -> Result<String, String> {
    let active_case_id = state
        .active_case
        .lock()
        .map_err(|_| "media handle unavailable".to_string())?
        .as_ref()
        .map(|active| active.meta.id.0.clone())
        .ok_or_else(|| "media handle unavailable".to_string())?;

    let cache = state
        .runtime_cache
        .lock()
        .map_err(|_| "media handle unavailable".to_string())?;
    let handle = cache
        .handles()
        .get(handle_id)
        .map_err(|_| "media handle unavailable".to_string())?
        .ok_or_else(|| "media handle expired or invalid".to_string())?;

    if handle.case_id != active_case_id {
        return Err("media handle expired or invalid".to_string());
    }

    Ok(handle.object_id)
}

pub fn resolve_media_handle_from_uri(uri: &http::Uri) -> Result<String, String> {
    let path = uri.path().trim_start_matches('/');
    let encoded = path
        .strip_prefix("handle/")
        .or_else(|| path.strip_prefix("/handle/"))
        .unwrap_or(path);

    if encoded.trim().is_empty() {
        return Err("missing media handle".to_string());
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "invalid media handle encoding".to_string())
        .and_then(|bytes| {
            String::from_utf8(bytes).map_err(|_| "invalid media handle encoding".to_string())
        })
}

pub fn parse_media_range_header(
    header_value: Option<&str>,
    total_size: u64,
    max_chunk: u64,
) -> Result<ResolvedRange, RangeError> {
    if total_size == 0 {
        return Err(RangeError::EmptyFile);
    }
    let max_chunk = max_chunk.max(1);
    let last_byte = total_size - 1;

    let Some(header_value) = header_value else {
        let end = last_byte.min(max_chunk - 1);
        return Ok(ResolvedRange {
            start: 0,
            end,
            length: end + 1,
            status: StatusCode::PARTIAL_CONTENT,
        });
    };

    let spec = header_value
        .trim()
        .strip_prefix("bytes=")
        .ok_or(RangeError::Invalid)?;
    if spec.contains(',') {
        return Err(RangeError::Invalid);
    }
    let (start_raw, end_raw) = spec.split_once('-').ok_or(RangeError::Invalid)?;

    let (start, requested_end) = if start_raw.is_empty() {
        let suffix_len = end_raw.parse::<u64>().map_err(|_| RangeError::Invalid)?;
        if suffix_len == 0 {
            return Err(RangeError::Invalid);
        }
        let bounded_suffix = suffix_len.min(total_size).min(max_chunk);
        (total_size - bounded_suffix, last_byte)
    } else {
        let start = start_raw.parse::<u64>().map_err(|_| RangeError::Invalid)?;
        if start >= total_size {
            return Err(RangeError::Unsatisfiable);
        }
        let requested_end = if end_raw.is_empty() {
            last_byte
        } else {
            end_raw.parse::<u64>().map_err(|_| RangeError::Invalid)?
        };
        if requested_end < start {
            return Err(RangeError::Invalid);
        }
        (start, requested_end.min(last_byte))
    };

    let max_end = start.saturating_add(max_chunk - 1);
    let end = requested_end.min(max_end).min(last_byte);
    Ok(ResolvedRange {
        start,
        end,
        length: end - start + 1,
        status: StatusCode::PARTIAL_CONTENT,
    })
}

pub fn build_content_range(range: &ResolvedRange, total_size: u64) -> String {
    format!("bytes {}-{}/{}", range.start, range.end, total_size)
}

pub fn register(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    builder.register_asynchronous_uri_scheme_protocol(
        EVIDENCE_MEDIA_SCHEME,
        |ctx, request, responder| {
            let app_handle = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let response = handle_media_protocol_request(app_handle, request);
                responder.respond(response);
            });
        },
    )
}

fn handle_media_protocol_request(
    app_handle: AppHandle<Wry>,
    request: http::Request<Vec<u8>>,
) -> http::Response<Cow<'static, [u8]>> {
    match handle_media_protocol_request_inner(app_handle, request) {
        Ok(response) => response,
        Err((status, message)) => text_response(status, &message),
    }
}

fn handle_media_protocol_request_inner(
    app_handle: AppHandle<Wry>,
    request: http::Request<Vec<u8>>,
) -> Result<http::Response<Cow<'static, [u8]>>, (StatusCode, String)> {
    let handle_id = resolve_media_handle_from_uri(request.uri())
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;
    let app_state = app_handle.state::<AppState>();
    let file_id = resolve_scoped_media_handle(app_state.inner(), &handle_id)
        .map_err(|err| (StatusCode::GONE, err))?;
    let range_header = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let app_state = app_state.inner().clone();
    let (case_id, db_path) = {
        let guard = app_state.active_case.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "media backend unavailable".to_string(),
            )
        })?;
        let active = guard.as_ref().ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "media handle unavailable".to_string(),
            )
        })?;
        (active.meta.id.0.clone(), active.db_path())
    };
    let conn = persistence_sqlite::open_or_create(&db_path).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "media backend unavailable".to_string(),
        )
    })?;

    let handle = with_preview_cache_context!(&app_state, &conn, case_id.as_str(), |context| {
        file_service::open_file_handle_real(context, &file_id)
    })
    .map_err(|_| (StatusCode::NOT_FOUND, "media file unavailable".to_string()))?;
    let range = parse_media_range_header(
        range_header.as_deref(),
        handle.size,
        MAX_MEDIA_PROTOCOL_READ_BYTES,
    )
    .map_err(|err| (err.status(), "invalid media range".to_string()))?;

    let bytes = read_media_protocol_bytes(
        &app_state,
        &conn,
        &case_id,
        &file_id,
        range.start,
        range.length.min(u32::MAX as u64) as u32,
    )?;
    let bytes_read = bytes.len();

    let content_end = range
        .start
        .saturating_add(bytes_read.saturating_sub(1) as u64);
    let content_range = if bytes_read == 0 {
        format!("bytes */{}", handle.size)
    } else {
        build_content_range(
            &ResolvedRange {
                end: content_end,
                length: bytes_read as u64,
                ..range
            },
            handle.size,
        )
    };

    http::Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(
            header::CONTENT_TYPE,
            handle.mime.as_deref().unwrap_or("application/octet-stream"),
        )
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_RANGE, content_range)
        .header(header::CONTENT_LENGTH, bytes_read.to_string())
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Range, Content-Length, Accept-Ranges",
        )
        .body(Cow::Owned(bytes))
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

fn read_media_protocol_bytes(
    state: &AppState,
    conn: &rusqlite::Connection,
    case_id: &str,
    file_id: &str,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, (StatusCode, String)> {
    if let Ok(path) = with_preview_cache_context!(state, conn, case_id, |context| {
        file_service::get_file_path_for_entry(context, file_id)
    }) {
        let mut file = std::fs::File::open(path).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "media backend unavailable".to_string(),
            )
        })?;
        file.seek(SeekFrom::Start(offset)).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "media backend unavailable".to_string(),
            )
        })?;
        let mut bytes = Vec::with_capacity(length as usize);
        file.take(length as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "media backend unavailable".to_string(),
                )
            })?;
        return Ok(bytes);
    }

    #[cfg(test)]
    MEDIA_PROTOCOL_BYTES_SERVICE_READ_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    with_preview_cache_context!(state, conn, case_id, |context| {
        file_service::read_file_bytes_for_case(
            context,
            &domain::FileEntryId(file_id.to_string()),
            offset,
            length,
        )
    })
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "media backend unavailable".to_string(),
        )
    })
}

fn text_response(status: StatusCode, message: &str) -> http::Response<Cow<'static, [u8]>> {
    http::Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Cow::Owned(message.as_bytes().to_vec()))
        .expect("text response should be valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};

    fn reset_media_protocol_bytes_service_read_call_count() {
        MEDIA_PROTOCOL_BYTES_SERVICE_READ_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    fn media_protocol_bytes_service_read_call_count() -> usize {
        MEDIA_PROTOCOL_BYTES_SERVICE_READ_CALLS.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn with_raw_exfat_case_file(
        test: impl FnOnce(
            &AppState,
            &rusqlite::Connection,
            String,
            String,
        ) -> Result<(), persistence_sqlite::DbError>,
    ) {
        let tmp = tempfile::TempDir::new().unwrap();
        let raw_path = tmp.path().join("exfat.raw");
        write_exfat_raw_fixture(&raw_path).unwrap();

        let conn = persistence_sqlite::open_or_create(&tmp.path().join("case.db")).unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-protocol-raw', 'Protocol Raw', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let case_id = domain::CaseId("case-protocol-raw".to_string());
        let ds_id = domain::DataSourceId("ds-protocol-raw-exfat".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "raw exfat evidence".to_string(),
                    kind: domain::DataSourceKind::Raw,
                    source_path: raw_path,
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )
            .unwrap();

        let file_id = domain::FileEntryId("file-protocol-raw-exfat".to_string());
        FileRepo::new(&conn)
            .insert_batch(&[domain::FileEntry {
                id: file_id.clone(),
                parent_id: None,
                data_source_id: ds_id,
                path: "LARGE.BIN".to_string(),
                name: "LARGE.BIN".to_string(),
                entry_type: domain::EntryType::File,
                size: Some(1536),
                ext: Some("bin".to_string()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            }])
            .unwrap();

        let state = AppState::default();
        test(&state, &conn, case_id.0, file_id.0).unwrap();
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
    fn protocol_url_encodes_opaque_handle() {
        let url = media_protocol_url("opaque-handle-123");
        assert_eq!(url, "evidence-media://handle/b3BhcXVlLWhhbmRsZS0xMjM");
    }

    #[test]
    fn protocol_mid_raw_image_range_reads_via_bytes_only_service_path() {
        with_raw_exfat_case_file(|state, conn, case_id, file_id| {
            reset_media_protocol_bytes_service_read_call_count();
            let bytes = read_media_protocol_bytes(state, conn, &case_id, &file_id, 512 + 7, 9)
                .map_err(|(_, message)| persistence_sqlite::DbError::System(message))?;

            assert_eq!(bytes, vec![b'B'; 9]);
            assert_eq!(media_protocol_bytes_service_read_call_count(), 1);

            Ok(())
        });
    }

    #[test]
    fn parse_range_bytes_start_end() {
        let range = parse_media_range_header(Some("bytes=10-19"), 100, 1024).unwrap();
        assert_eq!(range.start, 10);
        assert_eq!(range.end, 19);
        assert_eq!(range.length, 10);
        assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
    }

    #[test]
    fn parse_range_bytes_start_open() {
        let range = parse_media_range_header(Some("bytes=10-"), 100, 20).unwrap();
        assert_eq!(range.start, 10);
        assert_eq!(range.end, 29);
        assert_eq!(range.length, 20);
    }

    #[test]
    fn parse_range_suffix() {
        let range = parse_media_range_header(Some("bytes=-25"), 100, 10).unwrap();
        assert_eq!(range.start, 90);
        assert_eq!(range.end, 99);
        assert_eq!(range.length, 10);
    }

    #[test]
    fn parse_range_invalid_syntax_returns_416() {
        let err = parse_media_range_header(Some("items=0-1"), 100, 10).unwrap_err();
        assert_eq!(err.status(), StatusCode::RANGE_NOT_SATISFIABLE);

        let err = parse_media_range_header(Some("bytes=20-10"), 100, 10).unwrap_err();
        assert_eq!(err, RangeError::Invalid);
    }

    #[test]
    fn parse_range_out_of_bounds_returns_416() {
        let err = parse_media_range_header(Some("bytes=100-120"), 100, 10).unwrap_err();
        assert_eq!(err, RangeError::Unsatisfiable);
    }

    #[test]
    fn parse_range_no_header_is_bounded() {
        let range = parse_media_range_header(None, 10_000, 1024).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 1023);
        assert_eq!(range.length, 1024);
        assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
    }

    #[test]
    fn parse_range_zero_size() {
        let err = parse_media_range_header(None, 0, 1024).unwrap_err();
        assert_eq!(err, RangeError::EmptyFile);
    }

    #[test]
    fn parse_range_overflow_safe() {
        let range =
            parse_media_range_header(Some("bytes=18446744073709551614-"), u64::MAX, 1024).unwrap();
        assert_eq!(range.start, u64::MAX - 1);
        assert_eq!(range.end, u64::MAX - 1);
        assert_eq!(range.length, 1);
    }

    #[test]
    fn content_range_is_standard() {
        let range = ResolvedRange {
            start: 5,
            end: 9,
            length: 5,
            status: StatusCode::PARTIAL_CONTENT,
        };
        assert_eq!(build_content_range(&range, 20), "bytes 5-9/20");
    }

    // --- Task 1.2.1: Range boundary case hardening ---

    #[test]
    fn parse_range_single_byte_start_zero() {
        let range = parse_media_range_header(Some("bytes=0-0"), 100, 1024).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 0);
        assert_eq!(range.length, 1);
        assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
    }

    #[test]
    fn parse_range_single_byte_suffix_one() {
        let range = parse_media_range_header(Some("bytes=-1"), 100, 1024).unwrap();
        assert_eq!(range.start, 99);
        assert_eq!(range.end, 99);
        assert_eq!(range.length, 1);
        assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
    }

    #[test]
    fn parse_range_oversized_end_clamped_to_max_chunk() {
        let range =
            parse_media_range_header(Some("bytes=0-999999999"), 1_000_000_000, 1024).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 1023);
        assert_eq!(range.length, 1024);
    }

    #[test]
    fn parse_range_reverse_returns_error() {
        let err = parse_media_range_header(Some("bytes=100-50"), 200, 1024).unwrap_err();
        assert_eq!(err, RangeError::Invalid);
    }

    #[test]
    fn parse_range_suffix_larger_than_file_returns_entire_file() {
        let range = parse_media_range_header(Some("bytes=-999"), 100, 1024).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 99);
        assert_eq!(range.length, 100);
    }

    #[test]
    fn parse_range_suffix_zero_returns_error() {
        let err = parse_media_range_header(Some("bytes=-0"), 100, 1024).unwrap_err();
        assert_eq!(err, RangeError::Invalid);
    }

    #[test]
    fn parse_range_start_at_last_byte() {
        let range = parse_media_range_header(Some("bytes=99-99"), 100, 1024).unwrap();
        assert_eq!(range.start, 99);
        assert_eq!(range.end, 99);
        assert_eq!(range.length, 1);
    }

    #[test]
    fn parse_range_start_exactly_at_total_size_is_unsatisfiable() {
        let err = parse_media_range_header(Some("bytes=100-200"), 100, 1024).unwrap_err();
        assert_eq!(err, RangeError::Unsatisfiable);
    }

    #[test]
    fn parse_range_start_past_total_size_is_unsatisfiable() {
        let err = parse_media_range_header(Some("bytes=999-1000"), 100, 1024).unwrap_err();
        assert_eq!(err, RangeError::Unsatisfiable);
    }

    #[test]
    fn parse_range_multiple_ranges_rejected() {
        let err = parse_media_range_header(Some("bytes=0-10, 20-30"), 100, 1024).unwrap_err();
        assert_eq!(err, RangeError::Invalid);
    }

    #[test]
    fn parse_range_missing_dash_returns_error() {
        let err = parse_media_range_header(Some("bytes=010"), 100, 1024).unwrap_err();
        assert_eq!(err, RangeError::Invalid);
    }

    #[test]
    fn parse_range_max_chunk_of_one() {
        let range = parse_media_range_header(Some("bytes=0-99"), 100, 1).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 0);
        assert_eq!(range.length, 1);
    }

    #[test]
    fn parse_range_file_size_one_byte() {
        let range = parse_media_range_header(None, 1, 1024).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 0);
        assert_eq!(range.length, 1);

        let range = parse_media_range_header(Some("bytes=0-0"), 1, 1024).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 0);
        assert_eq!(range.length, 1);

        let err = parse_media_range_header(Some("bytes=1-1"), 1, 1024).unwrap_err();
        assert_eq!(err, RangeError::Unsatisfiable);
    }

    // --- Task 1.2.2: Concurrency safety (structural proof) ---

    #[test]
    fn parse_range_concurrent_independent_calls() {
        use std::thread;

        let total: u64 = 10_000_000;
        let max_chunk: u64 = 1024;
        let iterations = 100;

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let start_offset = (i as u64) * (total / 10);
                thread::spawn(move || {
                    for j in 0..iterations {
                        let offset = start_offset + j;
                        if offset >= total {
                            break;
                        }
                        let header = format!("bytes={}-", offset);
                        let range =
                            parse_media_range_header(Some(&header), total, max_chunk).unwrap();
                        assert_eq!(range.start, offset);
                        assert!(range.length <= max_chunk);
                        assert!(range.end < total);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread must not panic");
        }
    }
}
