//! Source-bound image metadata orchestration, shared by ordinary and recovered files.

use std::sync::atomic::AtomicBool;

use super::image_structure::{
    analyze_image, to_dto, ImageAnalyzeOptions, ImageByteSource, ImageStructureError,
};
use super::{open_file_handle_real, read_preview_bytes_for_file, PreviewReadContext};
use crate::file_service::FileServiceError;
use transport::dto::ImageMetadataDto;

pub fn image_metadata_for_file<C>(
    mut context: C,
    file_id: &str,
) -> Result<ImageMetadataDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;
    let mut source = PreviewImageSource {
        context: &mut context,
        file_id,
        size: handle.size,
    };
    let report = analyze_image(
        &mut source,
        &ImageAnalyzeOptions::viewer(),
        &AtomicBool::new(false),
    )
    .map_err(ImageStructureError::into_file_service_error)?;
    Ok(to_dto(report))
}

struct PreviewImageSource<'a, C> {
    context: &'a mut C,
    file_id: &'a str,
    size: u64,
}

impl<C: PreviewReadContext> ImageByteSource for PreviewImageSource<'_, C> {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, FileServiceError> {
        read_preview_bytes_for_file(&mut self.context, self.file_id, offset, length as u32)
    }
}
