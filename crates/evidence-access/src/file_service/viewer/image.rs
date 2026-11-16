use base64::Engine;
use std::sync::atomic::AtomicBool;
use transport::dto::{ImagePreviewDto, ViewerPreviewKindDto};

use crate::file_service::FileServiceError;

use super::image_structure::{analyze_image, ImageAnalyzeOptions, ImageByteSource};
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
    if handle.preview_kind != Some(ViewerPreviewKindDto::Image) {
        return Err(FileServiceError::Unsupported(
            "format has no inline preview".to_string(),
        ));
    }
    if handle.size > infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES {
        return Err(FileServiceError::Unsupported(format!(
            "Image preview is limited to {} MB",
            infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES
                / infrastructure::constants::BYTES_PER_MB
        )));
    }

    let content_bytes = read_inline_preview_bytes_for_file(&mut context, file_id, handle.size)?;
    let Some(dimensions) = image_dimensions(&content_bytes) else {
        return Err(FileServiceError::Unsupported(
            "image dimensions could not be determined".to_string(),
        ));
    };
    validate_dimensions(dimensions)?;
    let (width, height) = dimensions;
    let base64 = base64::engine::general_purpose::STANDARD.encode(&content_bytes);
    Ok(ImagePreviewDto {
        data_url: format!("data:{};base64,{}", mime, base64),
        mime_type: mime.to_string(),
        width,
        height,
        size: handle.size,
    })
}

fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut source = InlineImageSource(bytes);
    analyze_image(
        &mut source,
        &ImageAnalyzeOptions::dimensions(),
        &AtomicBool::new(false),
    )
    .ok()?
    .dimensions
}

fn validate_dimensions((width, height): (u32, u32)) -> Result<(), FileServiceError> {
    let limit = infrastructure::constants::MAX_IMAGE_PREVIEW_DIMENSION;
    let pixels = u64::from(width) * u64::from(height);
    if width > limit
        || height > limit
        || pixels > infrastructure::constants::MAX_IMAGE_PREVIEW_PIXELS
    {
        return Err(FileServiceError::Unsupported(
            "image dimensions exceed the inline preview limit".to_string(),
        ));
    }
    Ok(())
}

struct InlineImageSource<'a>(&'a [u8]);

impl ImageByteSource for InlineImageSource<'_> {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, FileServiceError> {
        let start = offset as usize;
        if start >= self.0.len() {
            return Ok(Vec::new());
        }
        let end = start.saturating_add(length).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }
}
