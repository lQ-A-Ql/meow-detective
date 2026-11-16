//! ICO directory traversal and declared-end checks.

use super::limits::ICO_ENTRY_LIMIT;
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_finding, push_segment, set_declared_end, ImageAnalysisReport,
    ImageAnalyzeOptions, ImageIssueSeverity,
};

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let size = reader.size();
    if size < 6 {
        return Err(ScanError::Corrupt);
    }
    let header = reader.read(0, 6)?;
    let file_type = u16::from_le_bytes([header[2], header[3]]);
    let count = usize::from(u16::from_le_bytes([header[4], header[5]]));
    if file_type != 1 || count == 0 {
        return Err(ScanError::Corrupt);
    }
    if count > ICO_ENTRY_LIMIT {
        push_finding(
            report,
            "image.ico.entry-limit",
            ImageIssueSeverity::Warning,
            Some(4),
            Some(format!("the ICO directory declares {count} entries")),
        );
        return Err(ScanError::Limit);
    }
    let directory_end = 6u64 + (count as u64) * 16;
    if directory_end > size {
        mark_truncated(report, "image.truncated", size);
        return Ok(());
    }
    let directory = reader.read(6, count * 16)?;
    let mut max_width = 0u32;
    let mut max_height = 0u32;
    let mut max_end = directory_end;
    let mut truncated = false;
    for index in 0..count {
        let entry = &directory[index * 16..index * 16 + 16];
        let width = if entry[0] == 0 {
            256
        } else {
            u32::from(entry[0])
        };
        let height = if entry[1] == 0 {
            256
        } else {
            u32::from(entry[1])
        };
        max_width = max_width.max(width);
        max_height = max_height.max(height);
        let bytes_in_resource = u64::from(u32::from_le_bytes([
            entry[8], entry[9], entry[10], entry[11],
        ]));
        let image_offset = u64::from(u32::from_le_bytes([
            entry[12], entry[13], entry[14], entry[15],
        ]));
        let end = image_offset
            .checked_add(bytes_in_resource)
            .ok_or(ScanError::Corrupt)?;
        push_segment(
            report,
            "ico.image".to_string(),
            image_offset,
            bytes_in_resource,
            Some(format!("{width}x{height}")),
        );
        if end > size {
            truncated = true;
        }
        max_end = max_end.max(end);
    }
    report.dimensions = Some((max_width, max_height));
    if options.dimensions_only {
        return Ok(());
    }
    if truncated || max_end > size {
        mark_truncated(report, "image.truncated", size);
    } else {
        set_declared_end(report, max_end);
    }
    Ok(())
}
