//! WebP RIFF chunk traversal and dimension extraction.

use super::exif;
use super::limits::{EXIF_BLOCK_READ_LIMIT, SEGMENT_LIMIT};
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
    if size < 12 {
        return Err(ScanError::Corrupt);
    }
    let header = reader.read(0, 12)?;
    let riff_size = u32::from_le_bytes(header[4..8].try_into().map_err(|_| ScanError::Corrupt)?);
    let declared_end = 8u64 + u64::from(riff_size);
    if declared_end < 12 {
        return Err(ScanError::Corrupt);
    }
    let truncated = declared_end > size;
    let parse_end = declared_end.min(size);
    let mut offset = 12u64;
    let mut index = 0usize;
    while offset + 8 <= parse_end {
        if index >= SEGMENT_LIMIT {
            push_finding(
                report,
                "image.segment-limit",
                ImageIssueSeverity::Warning,
                Some(offset),
                Some("the WebP chunk limit was reached".to_string()),
            );
            return Err(ScanError::Limit);
        }
        let chunk = reader.read(offset, 8)?;
        let fourcc: [u8; 4] = chunk[..4].try_into().map_err(|_| ScanError::Corrupt)?;
        let length =
            u32::from_le_bytes(chunk[4..8].try_into().map_err(|_| ScanError::Corrupt)?) as usize;
        let padded = length.checked_add(length % 2).ok_or(ScanError::Corrupt)?;
        let next = offset
            .checked_add(8)
            .and_then(|value| value.checked_add(padded as u64))
            .ok_or(ScanError::Corrupt)?;
        if next > parse_end {
            break;
        }
        push_segment(
            report,
            format!("webp.{}", String::from_utf8_lossy(&fourcc)),
            offset,
            next - offset,
            None,
        );
        inspect_chunk(reader, report, offset, &fourcc, length)?;
        if options.dimensions_only && report.dimensions.is_some() {
            return Ok(());
        }
        offset = next;
        index += 1;
    }
    if truncated {
        mark_truncated(report, "image.truncated", offset);
    } else {
        set_declared_end(report, declared_end);
    }
    Ok(())
}

fn inspect_chunk(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    offset: u64,
    fourcc: &[u8; 4],
    length: usize,
) -> Result<(), ScanError> {
    match fourcc {
        b"EXIF" => {
            if length > EXIF_BLOCK_READ_LIMIT {
                exif::record_skipped_block(report, "webp.exif", offset, length as u64);
            } else if length > 0 {
                let payload = reader.read(offset + 8, length)?;
                let bytes = payload.strip_prefix(b"Exif\0\0").unwrap_or(&payload);
                exif::decode_block(report, "webp.exif", offset + 8, bytes.to_vec());
            }
        }
        b"VP8X" if length == 10 => {
            let bytes = reader.read(offset + 8, 10)?;
            report.dimensions = Some((
                1 + u32::from_le_bytes([bytes[4], bytes[5], bytes[6], 0]),
                1 + u32::from_le_bytes([bytes[7], bytes[8], bytes[9], 0]),
            ));
        }
        b"VP8 " if length >= 10 => {
            let bytes = reader.read(offset + 8, 10)?;
            if bytes[3..6] == [0x9d, 0x01, 0x2a] {
                report.dimensions = Some((
                    u32::from(u16::from_le_bytes([bytes[6], bytes[7]]) & 0x3fff),
                    u32::from(u16::from_le_bytes([bytes[8], bytes[9]]) & 0x3fff),
                ));
            }
        }
        b"VP8L" if length >= 5 => {
            let bytes = reader.read(offset + 8, 5)?;
            if bytes[0] == 0x2f {
                let bits = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
                report.dimensions = Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1));
            }
        }
        _ => {}
    }
    Ok(())
}
