//! Source-bound image metadata orchestration, shared by ordinary and recovered files.

use super::image_metadata_input::{MetadataInputError, MetadataReader};
use super::{open_file_handle_real, read_preview_bytes_for_file, PreviewReadContext};
use crate::file_service::FileServiceError;
use transport::dto::{ImageMetadataDto, ImageMetadataStatusDto};

pub fn image_metadata_for_file<C>(
    mut context: C,
    file_id: &str,
) -> Result<ImageMetadataDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;
    metadata_from_reader(handle.size, |offset, length| {
        read_preview_bytes_for_file(&mut context, file_id, offset, length)
    })
}

fn metadata_from_reader(
    size: u64,
    read: impl FnMut(u64, u32) -> Result<Vec<u8>, FileServiceError>,
) -> Result<ImageMetadataDto, FileServiceError> {
    let mut output = empty(ImageMetadataStatusDto::Absent);
    let input = match MetadataReader::new(size, read).container() {
        Ok(Some(input)) => input,
        Ok(None) => {
            output.status = ImageMetadataStatusDto::Unsupported;
            return Ok(output);
        }
        Err(error) => {
            output.status = error_status(error)?;
            return Ok(output);
        }
    };
    output.format = Some(input.format.to_owned());
    output.width = input.dimensions.map(|value| value.0);
    output.height = input.dimensions.map(|value| value.1);
    if let Some(bytes) = input.exif {
        output.status = match super::image_metadata_exif::decode(bytes, &mut output) {
            Ok(()) => ImageMetadataStatusDto::Present,
            Err(MetadataInputError::Corrupt) if input.incomplete => {
                ImageMetadataStatusDto::Oversized
            }
            Err(error) => error_status(error)?,
        };
        if output.status != ImageMetadataStatusDto::Present {
            output = empty(output.status);
            output.format = Some(input.format.to_owned());
            output.width = input.dimensions.map(|value| value.0);
            output.height = input.dimensions.map(|value| value.1);
        }
    }
    Ok(output)
}

fn error_status(error: MetadataInputError) -> Result<ImageMetadataStatusDto, FileServiceError> {
    match error {
        MetadataInputError::Corrupt => Ok(ImageMetadataStatusDto::Corrupt),
        MetadataInputError::Limit => Ok(ImageMetadataStatusDto::Oversized),
        MetadataInputError::Read(error) => Err(error),
    }
}

fn empty(status: ImageMetadataStatusDto) -> ImageMetadataDto {
    ImageMetadataDto {
        status,
        format: None,
        width: None,
        height: None,
        orientation: None,
        make: None,
        model: None,
        software: None,
        date_time_original: None,
        create_date: None,
        modify_date: None,
        lens_model: None,
        latitude: None,
        longitude: None,
        altitude: None,
        gps_date_time: None,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/image_metadata.rs"]
mod tests;
