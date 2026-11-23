//! Bounded ICO structure inspection through the shared native directory reader.

use image_inspection::ico_codec::{IconDir, ResourceType};
use std::io::{Cursor, ErrorKind};

use super::limits::ICO_ENTRY_LIMIT;
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_segment, set_declared_end, ImageAnalysisReport, ImageAnalyzeOptions,
};

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let size = reader.size();
    let prefix = reader.read(0, size.min((6 + 16 * ICO_ENTRY_LIMIT) as u64) as usize)?;
    let metadata = match IconDir::read_metadata(Cursor::new(prefix), ICO_ENTRY_LIMIT) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::FileTooLarge => return Err(ScanError::Limit),
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
            mark_truncated(report, "image.truncated", size);
            return Ok(());
        }
        Err(_) => return Err(ScanError::Corrupt),
    };
    if metadata.resource_type != ResourceType::Icon || metadata.entries.is_empty() {
        return Err(ScanError::Corrupt);
    }
    let directory_end = 6 + 16 * metadata.entries.len() as u64;
    let mut max_end = directory_end;
    let mut max_width = 0;
    let mut max_height = 0;
    for entry in metadata.entries {
        let offset = u64::from(entry.offset);
        let length = u64::from(entry.length);
        if offset < directory_end || length == 0 {
            return Err(ScanError::Corrupt);
        }
        max_width = max_width.max(entry.width);
        max_height = max_height.max(entry.height);
        max_end = max_end.max(offset + length);
        push_segment(
            report,
            "ico.image".into(),
            offset,
            length,
            Some(format!("{}x{}", entry.width, entry.height)),
        );
    }
    report.dimensions = Some((max_width, max_height));
    if options.dimensions_only {
        return Ok(());
    }
    if max_end > size {
        mark_truncated(report, "image.truncated", size);
    } else {
        set_declared_end(report, max_end);
    }
    Ok(())
}
