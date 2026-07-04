//! Tauri-free preview facade for text, image, and media DTO assembly.

use crate::{
    file_service::{
        get_file_path_for_entry, open_file_handle_real, read_file_bytes_for_case, FileServiceError,
    },
    text_service::TextService,
};
use base64::Engine;
use domain::FileEntryId;
use std::io::{Read, Seek, SeekFrom};
use transport::dto::{
    ImagePreviewDto, MediaPreviewModeDto, MediaRangeRequestDto, MediaRangeResponseDto, MediaUrlDto,
    TextPreviewDto,
};

use super::viewer::PreviewReadContext;

#[derive(Debug, Clone)]
pub enum MediaPreviewPlan {
    Inline(MediaUrlDto),
    Protocol {
        mime_type: String,
        size: u64,
        can_read_ranges: bool,
    },
}

pub fn text_preview_for_file<C>(
    mut context: C,
    file_id: &str,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let max = max_bytes
        .unwrap_or(infrastructure::constants::DEFAULT_TEXT_PREVIEW_MAX_BYTES)
        .min(transport::dto::MAX_VIEWER_RANGE_LENGTH as usize) as u32;
    let content_bytes = read_preview_bytes_for_file(&mut context, file_id, 0, max)?;

    let preview =
        TextService::extract_text_preview(&mut std::io::Cursor::new(&content_bytes), max as usize)?;

    let is_binary = preview.is_binary;
    let content = preview.content;
    let hex_dump = if is_binary {
        Some(format_hex_dump(&content_bytes))
    } else {
        None
    };

    Ok(TextPreviewDto {
        hex_dump,
        content,
        encoding: preview.encoding,
        is_truncated: preview.is_truncated,
        line_count: preview.line_count,
        is_binary,
        language: preview.language,
    })
}

pub fn image_preview_for_file<C>(
    mut context: C,
    file_id: &str,
) -> Result<ImagePreviewDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;

    let mime = handle.mime.as_deref().unwrap_or("");
    if !mime.starts_with("image/") {
        return Err(FileServiceError::invalid_input("Not an image file"));
    }

    if handle.size > infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES {
        return Err(FileServiceError::invalid_input(format!(
            "Image preview is limited to {} MB",
            infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES
                / infrastructure::constants::BYTES_PER_MB
        )));
    }

    let content_bytes = read_inline_preview_bytes_for_file(&mut context, file_id, handle.size)?;
    let base64 = base64::engine::general_purpose::STANDARD.encode(&content_bytes);

    Ok(ImagePreviewDto {
        data_url: format!("data:{};base64,{}", mime, base64),
        mime_type: mime.to_string(),
        width: 0,
        height: 0,
        size: handle.size,
    })
}

pub fn media_preview_plan_for_file<C>(
    mut context: C,
    file_id: &str,
) -> Result<MediaPreviewPlan, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;
    let mime = handle
        .mime
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());

    if handle.size > infrastructure::constants::MAX_INLINE_MEDIA_PREVIEW_BYTES {
        return Ok(MediaPreviewPlan::Protocol {
            mime_type: mime,
            size: handle.size,
            can_read_ranges: true,
        });
    }

    let content_bytes = read_inline_preview_bytes_for_file(&mut context, file_id, handle.size)?;
    let base64 = base64::engine::general_purpose::STANDARD.encode(&content_bytes);

    Ok(MediaPreviewPlan::Inline(MediaUrlDto {
        mode: MediaPreviewModeDto::Inline,
        url: Some(format!("data:{};base64,{}", mime, base64)),
        handle_id: Some(handle.handle_id),
        mime_type: mime,
        size: handle.size,
        can_read_ranges: true,
    }))
}

pub fn media_range_for_file<C>(
    mut context: C,
    file_id: &str,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;
    if request.offset >= handle.size {
        return Ok(MediaRangeResponseDto {
            offset: request.offset,
            bytes_base64: String::new(),
            bytes_read: 0,
            eof: true,
        });
    }

    let readable_len = request
        .length
        .min((handle.size - request.offset).min(u32::MAX as u64) as u32);
    let bytes = read_preview_bytes_for_file(&mut context, file_id, request.offset, readable_len)?;
    let bytes_read = bytes.len();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let end_offset = request.offset.saturating_add(bytes_read as u64);

    Ok(MediaRangeResponseDto {
        offset: request.offset,
        bytes_base64: encoded,
        bytes_read: bytes_read as u32,
        eof: end_offset >= handle.size,
    })
}

pub fn read_preview_bytes_for_file<C>(
    mut context: C,
    file_id: &str,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError>
where
    C: PreviewReadContext,
{
    if let Ok(path) = get_file_path_for_entry(&mut context, file_id) {
        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::with_capacity(length as usize);
        file.take(length as u64).read_to_end(&mut bytes)?;
        return Ok(bytes);
    }

    read_file_bytes_for_case(
        &mut context,
        &FileEntryId(file_id.to_string()),
        offset,
        length,
    )
}

fn read_inline_preview_bytes_for_file<C>(
    context: &mut C,
    file_id: &str,
    size: u64,
) -> Result<Vec<u8>, FileServiceError>
where
    C: PreviewReadContext,
{
    let mut bytes = Vec::with_capacity(size as usize);
    let mut offset = 0u64;

    while offset < size {
        let length = (size - offset).min(transport::dto::MAX_VIEWER_RANGE_LENGTH as u64) as u32;
        if length == 0 {
            break;
        }

        let chunk = read_preview_bytes_for_file(&mut *context, file_id, offset, length)?;
        if chunk.is_empty() {
            break;
        }

        let is_short_read = chunk.len() < length as usize;
        offset = offset.saturating_add(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);

        if is_short_read {
            break;
        }
    }

    Ok(bytes)
}

fn format_hex_dump(bytes: &[u8]) -> String {
    let max_display = 16384usize.min(bytes.len());
    let mut out = String::with_capacity(max_display * 5);
    for (line_idx, chunk) in bytes[..max_display].chunks(16).enumerate() {
        let offset = line_idx * 16;
        use std::fmt::Write;
        let _ = write!(out, "{offset:08X}  ");
        for (i, b) in chunk.iter().enumerate() {
            if i == 8 {
                out.push(' ');
            }
            let _ = write!(out, "{b:02X} ");
        }
        out.push_str(" |");
        for b in chunk {
            out.push(if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            });
        }
        out.push_str("|\n");
    }
    if bytes.len() > max_display {
        out.push_str("... (truncated)\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{case_service, file_service};
    use evidence_core::LogicalFsReader;
    use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
    use tempfile::TempDir;

    fn read_file_bytes_for_case_call_count() -> usize {
        crate::file_service::viewer::READ_FILE_BYTES_FOR_CASE_CALLS.with(|calls| calls.get())
    }

    fn reset_read_file_bytes_for_case_call_count() {
        crate::file_service::viewer::READ_FILE_BYTES_FOR_CASE_CALLS.with(|calls| calls.set(0));
    }

    fn with_logical_case_file(
        case_name: &str,
        file_name: &str,
        content: &[u8],
        test: impl FnOnce(&rusqlite::Connection, String) -> Result<(), persistence_sqlite::DbError>,
    ) {
        let tmp = TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        std::fs::write(evidence_dir.join(file_name), content).unwrap();

        let active =
            case_service::create_case(&tmp.path().join("cases"), case_name, Some("tester"))
                .unwrap();
        let case_id = active.meta.id.clone();

        active
            .with_conn(|conn| {
                let ds_id = domain::DataSourceId("ds-preview".to_string());
                DataSourceRepo::new(conn).insert(
                    &case_id,
                    &domain::DataSource {
                        id: ds_id.clone(),
                        name: "evidence".to_string(),
                        kind: domain::DataSourceKind::LogicalDirectory,
                        source_path: evidence_dir.clone(),
                        imported_at: chrono::Utc::now(),
                        provenance: domain::DataSourceProvenance::unknown(),
                    },
                )?;

                let fs = LogicalFsReader::open(&evidence_dir, "evidence")
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;
                file_service::enumerate_filesystem(conn, &ds_id, &fs)?;

                let file_id = persistence_sqlite::repositories::file_repo::FileRepo::new(conn)
                    .find_by_data_source(&ds_id)?
                    .into_iter()
                    .find(|entry| entry.name == file_name)
                    .map(|entry| entry.id.0)
                    .expect("file should be enumerated");

                test(conn, file_id)
            })
            .unwrap();
    }

    #[test]
    fn text_preview_assembles_dto_from_service_bytes() {
        with_logical_case_file(
            "text-preview",
            "note.txt",
            b"hello\nworld",
            |conn, file_id| {
                let preview = text_preview_for_file(conn, &file_id, None)
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;

                assert_eq!(preview.content, "hello\nworld");
                assert_eq!(preview.encoding, "UTF-8");
                assert_eq!(preview.line_count, 2);
                assert!(!preview.is_binary);
                assert!(preview.hex_dump.is_none());

                Ok(())
            },
        );
    }

    #[test]
    fn image_preview_uses_direct_logical_path_without_range_fallback() {
        with_logical_case_file(
            "image-preview",
            "tiny.png",
            b"tiny image bytes",
            |conn, file_id| {
                reset_read_file_bytes_for_case_call_count();

                let image = image_preview_for_file(conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;

                assert_eq!(image.mime_type, "image/png");
                assert_eq!(read_file_bytes_for_case_call_count(), 0);
                let (_, encoded) = image.data_url.split_once(',').expect("data URL payload");
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded.as_bytes())
                        .unwrap(),
                    b"tiny image bytes"
                );

                Ok(())
            },
        );
    }

    #[test]
    fn oversized_media_preview_returns_protocol_plan_without_host_path() {
        let oversized =
            vec![b'A'; infrastructure::constants::MAX_INLINE_MEDIA_PREVIEW_BYTES as usize + 1];
        with_logical_case_file(
            "large-media-preview",
            "large.mp4",
            &oversized,
            |conn, file_id| {
                let plan = media_preview_plan_for_file(conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;

                let MediaPreviewPlan::Protocol {
                    mime_type,
                    size,
                    can_read_ranges,
                } = plan
                else {
                    panic!("large media should use protocol delivery");
                };
                assert_eq!(mime_type, "application/octet-stream");
                assert_eq!(size, oversized.len() as u64);
                assert!(can_read_ranges);

                Ok(())
            },
        );
    }

    #[test]
    fn media_range_returns_base64_bytes() {
        let content: Vec<u8> = (0u8..64).collect();
        with_logical_case_file(
            "media-range-preview",
            "clip.mp4",
            &content,
            |conn, file_id| {
                let request = MediaRangeRequestDto {
                    handle_id: "scoped-handle-owned-by-tauri".to_string(),
                    offset: 17,
                    length: 12,
                };

                let range = media_range_for_file(conn, &file_id, &request)
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;

                assert_eq!(range.offset, 17);
                assert_eq!(range.bytes_read, 12);
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(range.bytes_base64.as_bytes())
                        .unwrap(),
                    content[17..29].to_vec()
                );
                assert!(!range.eof);

                Ok(())
            },
        );
    }
}
