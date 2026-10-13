use super::RegistryBrowserError;
use crate::file_service::{RangeContentReader, SourceReadContext};
use domain::FileEntryId;
use std::io::Read;

pub(super) fn read_snapshot(
    context: &mut SourceReadContext<'_>,
    id: &FileEntryId,
    size: usize,
    capacity: usize,
) -> Result<Vec<u8>, RegistryBrowserError> {
    if capacity < size {
        return Err(RegistryBrowserError::InvalidRequest);
    }
    if size == 0 {
        return Ok(Vec::new());
    }
    let source = context
        .open_file_range_by_id(id)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    let mut reader: Box<dyn Read> = match source {
        RangeContentReader::Seekable(reader) => reader,
        RangeContentReader::Streaming(reader) => reader,
    };
    let mut bytes = vec![0; capacity];
    bytes.truncate(size);
    reader
        .read_exact(&mut bytes)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    Ok(bytes)
}
