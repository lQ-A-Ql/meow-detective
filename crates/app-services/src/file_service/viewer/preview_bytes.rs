use std::io::{Read, Seek, SeekFrom};

use domain::FileEntryId;

use crate::file_service::{get_file_path_for_entry, read_file_bytes_for_case, FileServiceError};

use super::PreviewReadContext;

pub fn read_preview_bytes_for_file<C>(
    mut context: C,
    file_id: &str,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError>
where
    C: PreviewReadContext,
{
    let length = length.min(transport::dto::MAX_VIEWER_RANGE_LENGTH);
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

pub(super) fn read_inline_preview_bytes_for_file<C>(
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
