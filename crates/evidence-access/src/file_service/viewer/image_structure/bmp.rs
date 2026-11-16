//! BMP header and declared-size checks.

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
    if size < 18 {
        return Err(ScanError::Corrupt);
    }
    let header = reader.read(0, size.min(34) as usize)?;
    if !header.starts_with(b"BM") || header.len() < 18 {
        return Err(ScanError::Corrupt);
    }
    let file_size = u64::from(u32::from_le_bytes([
        header[2], header[3], header[4], header[5],
    ]));
    let pixel_offset = u64::from(u32::from_le_bytes([
        header[10], header[11], header[12], header[13],
    ]));
    let dib_size = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);
    if dib_size < 40 {
        return Err(ScanError::Corrupt);
    }
    if header.len() < 30 {
        mark_truncated(report, "image.truncated", size);
        return Ok(());
    }
    let width = i32::from_le_bytes([header[18], header[19], header[20], header[21]]);
    let height = i32::from_le_bytes([header[22], header[23], header[24], header[25]]);
    let bit_count = u16::from_le_bytes([header[28], header[29]]);
    if width <= 0 || height == 0 || bit_count == 0 {
        return Err(ScanError::Corrupt);
    }
    let width = u64::try_from(width).map_err(|_| ScanError::Corrupt)?;
    let height = u64::from(height.unsigned_abs());
    report.dimensions = Some((
        u32::try_from(width).map_err(|_| ScanError::Corrupt)?,
        u32::try_from(height).map_err(|_| ScanError::Corrupt)?,
    ));
    if options.dimensions_only {
        return Ok(());
    }
    push_segment(report, "bmp.header".to_string(), 0, pixel_offset, None);
    let row_size = (width * u64::from(bit_count)).div_ceil(32) * 4;
    let pixel_end = pixel_offset
        .checked_add(row_size.checked_mul(height).ok_or(ScanError::Corrupt)?)
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
    if file_size > size {
        mark_truncated(report, "image.truncated", size);
    }
    Ok(())
}
