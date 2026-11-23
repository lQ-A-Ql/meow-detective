//! BMP header and declared-size checks.

use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_finding, push_segment, set_declared_end, ImageAnalysisReport,
    ImageAnalyzeOptions, ImageIssueSeverity,
};
use image_inspection::bmp::Header;

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let size = reader.size();
    let prefix = reader.read(0, size.min(14 + 124) as usize)?;
    let header = Header::from_slice(&prefix).map_err(|_| ScanError::Corrupt)?;
    let file_size = u64::from(header.file_size);
    let pixel_offset = header.image_data_start as u64;
    report.dimensions = Some((header.image_size.width, header.image_size.height));
    if options.dimensions_only {
        return Ok(());
    }
    push_segment(report, "bmp.header".to_string(), 0, pixel_offset, None);
    let pixel_end = pixel_offset
        .checked_add(header.image_data_length().ok_or(ScanError::Corrupt)? as u64)
        .ok_or(ScanError::Corrupt)?;
    push_segment(
        report,
        "bmp.pixels".to_string(),
        pixel_offset,
        pixel_end.saturating_sub(pixel_offset),
        None,
    );
    if file_size == 0 {
        push_finding(
            report,
            "image.bmp.size-field-zero",
            ImageIssueSeverity::Info,
            Some(2),
            Some("the BMP file-size field is zero".to_string()),
        );
        if pixel_end > size {
            mark_truncated(report, "image.truncated", size);
        }
        return Ok(());
    }
    if file_size < pixel_offset {
        return Err(ScanError::Corrupt);
    }
    if file_size != pixel_end {
        push_finding(
            report,
            "image.bmp.size-mismatch",
            ImageIssueSeverity::Warning,
            Some(2),
            Some(format!(
                "the BMP header declares {file_size} bytes but the pixel layout ends at {pixel_end}"
            )),
        );
    }
    set_declared_end(report, file_size);
    if file_size > size || pixel_end > size {
        mark_truncated(report, "image.truncated", size);
    }
    Ok(())
}
