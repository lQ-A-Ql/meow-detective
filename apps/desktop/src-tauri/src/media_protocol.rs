use crate::state::AppState;
use app_services::file_service;
use base64::Engine;
use chrono::Duration;
use std::borrow::Cow;
use std::io::Read;
use tauri::http::{self, header, StatusCode};
use tauri::{AppHandle, Manager, Wry};

pub const EVIDENCE_MEDIA_SCHEME: &str = "evidence-media";
pub const MAX_MEDIA_PROTOCOL_READ_BYTES: u64 = transport::dto::MAX_VIEWER_RANGE_LENGTH as u64;
const MEDIA_HANDLE_TTL_MINUTES: i64 = 30;

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
    let db_path = {
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
        active.db_path()
    };
    let conn = persistence_sqlite::open_or_create(&db_path).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "media backend unavailable".to_string(),
        )
    })?;

    let handle = file_service::open_file_handle_real(&conn, &file_id)
        .map_err(|_| (StatusCode::NOT_FOUND, "media file unavailable".to_string()))?;
    let range = parse_media_range_header(
        range_header.as_deref(),
        handle.size,
        MAX_MEDIA_PROTOCOL_READ_BYTES,
    )
    .map_err(|err| (err.status(), "invalid media range".to_string()))?;

    let mut reader = file_service::open_file_content_by_id(&conn, &domain::FileEntryId(file_id))
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "media backend unavailable".to_string(),
            )
        })?;
    file_service::skip_reader_bytes(reader.as_mut(), range.start).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "media backend unavailable".to_string(),
        )
    })?;

    let mut bytes = vec![0u8; range.length as usize];
    let bytes_read = reader.read(&mut bytes).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "media backend unavailable".to_string(),
        )
    })?;
    bytes.truncate(bytes_read);

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

    #[test]
    fn protocol_url_encodes_opaque_handle() {
        let url = media_protocol_url("opaque-handle-123");
        assert_eq!(url, "evidence-media://handle/b3BhcXVlLWhhbmRsZS0xMjM");
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
