use base64::Engine;
use transport::dto::ImagePreviewDto;

use crate::file_service::FileServiceError;

use super::{
    open_file_handle_real, preview_bytes::read_inline_preview_bytes_for_file, PreviewReadContext,
};

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
