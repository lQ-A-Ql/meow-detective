//! GIF block traversal with trailer and sub-block bounds.

use super::limits::{GIF_SUB_BLOCK_LIMIT, SEGMENT_LIMIT};
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
    let header = reader.read(0, 13)?;
    report.dimensions = Some((
        u32::from(u16::from_le_bytes([header[6], header[7]])),
        u32::from(u16::from_le_bytes([header[8], header[9]])),
    ));
    if options.dimensions_only {
        return Ok(());
    }
    let packed = header[10];
    let mut offset = 13u64;
    if packed & 0x80 != 0 {
        offset += 3 * (1u64 << ((packed & 0x07) + 1));
        if offset > size {
            mark_truncated(report, "image.truncated", 13);
            return Ok(());
        }
    }
    let mut index = 0usize;
    loop {
        if index >= SEGMENT_LIMIT {
            push_finding(
                report,
                "image.segment-limit",
                ImageIssueSeverity::Warning,
                Some(offset),
                Some("the GIF block limit was reached".to_string()),
            );
            return Err(ScanError::Limit);
        }
        if offset >= size {
            mark_truncated(report, "image.gif.missing-trailer", offset);
            return Ok(());
        }
        match reader.read(offset, 1)?[0] {
            0x3b => {
                push_segment(report, "gif.trailer".to_string(), offset, 1, None);
                set_declared_end(report, offset + 1);
                return Ok(());
            }
            0x21 => {
                if offset + 2 > size {
                    mark_truncated(report, "image.truncated", offset);
                    return Ok(());
                }
                let label = reader.read(offset + 1, 1)?[0];
                push_segment(
                    report,
                    format!("gif.extension.{label:02x}"),
                    offset,
                    2,
                    None,
                );
                let Some(next) = skip_sub_blocks(reader, offset + 2, size)? else {
                    mark_truncated(report, "image.truncated", offset);
                    return Ok(());
                };
                offset = next;
            }
            0x2c => {
                if offset + 10 > size {
                    mark_truncated(report, "image.truncated", offset);
                    return Ok(());
                }
                let descriptor = reader.read(offset + 1, 9)?;
                let packed = descriptor[8];
                let mut next = offset + 10;
                if packed & 0x80 != 0 {
                    next += 3 * (1u64 << ((packed & 0x07) + 1));
                }
                if next >= size {
                    mark_truncated(report, "image.truncated", offset);
                    return Ok(());
                }
                next += 1;
                push_segment(report, "gif.image".to_string(), offset, next - offset, None);
                let Some(after) = skip_sub_blocks(reader, next, size)? else {
                    mark_truncated(report, "image.truncated", offset);
                    return Ok(());
                };
                offset = after;
            }
            _ => return Err(ScanError::Corrupt),
        }
        index += 1;
    }
}

fn skip_sub_blocks(
    reader: &mut ScanReader<'_>,
    mut offset: u64,
    size: u64,
) -> Result<Option<u64>, ScanError> {
    for _ in 0..GIF_SUB_BLOCK_LIMIT {
        if offset >= size {
            return Ok(None);
        }
        let length = u64::from(reader.read(offset, 1)?[0]);
        if length == 0 {
            return Ok(Some(offset + 1));
        }
        offset = offset.checked_add(1 + length).ok_or(ScanError::Corrupt)?;
        if offset > size {
            return Ok(None);
        }
    }
    Err(ScanError::Limit)
}
