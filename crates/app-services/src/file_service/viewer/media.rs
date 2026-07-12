use base64::Engine;
use transport::dto::{
    MediaPreviewModeDto, MediaRangeRequestDto, MediaRangeResponseDto, MediaUrlDto,
};

use crate::file_service::FileServiceError;

use super::{
    open_file_handle_real,
    preview_bytes::{read_inline_preview_bytes_for_file, read_preview_bytes_for_file},
    PreviewReadContext,
};

#[derive(Debug, Clone)]
pub enum MediaPreviewPlan {
    Inline(MediaUrlDto),
    Protocol {
        mime_type: String,
        size: u64,
        can_read_ranges: bool,
    },
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
        .min(transport::dto::MAX_VIEWER_RANGE_LENGTH)
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
