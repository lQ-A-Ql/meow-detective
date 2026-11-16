//! PNG chunk traversal with IHDR/eXIf CRC checks and trailing-byte detection.

use super::exif;
use super::limits::{EXIF_BLOCK_READ_LIMIT, SEGMENT_LIMIT};
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_finding, push_segment, push_warning, set_declared_end,
    ImageAnalysisReport, ImageAnalyzeOptions, ImageIssueSeverity,
};

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let size = reader.size();
    let mut offset = 8u64;
    let mut saw_ihdr = false;
    let mut index = 0usize;
    while index < SEGMENT_LIMIT {
        if offset + 8 > size {
            mark_truncated(report, "image.png.missing-iend", offset);
            return Ok(());
        }
        let header = reader.read(offset, 8)?;
        let length =
            u32::from_be_bytes(header[..4].try_into().map_err(|_| ScanError::Corrupt)?) as usize;
        let kind: [u8; 4] = header[4..8].try_into().map_err(|_| ScanError::Corrupt)?;
        let end = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(length as u64))
            .ok_or(ScanError::Corrupt)?;
        if end > size {
            mark_truncated(report, "image.truncated", offset);
            return Ok(());
        }
        push_segment(
            report,
            format!("png.{}", String::from_utf8_lossy(&kind)),
            offset,
            end - offset,
            None,
        );
        if index == 0 && (&kind != b"IHDR" || length != 13) {
            return Err(ScanError::Corrupt);
        }
        if &kind == b"IHDR" {
            if saw_ihdr {
                return Err(ScanError::Corrupt);
            }
            saw_ihdr = true;
            match verified_payload(reader, offset, length, &kind)? {
                Some(payload) => {
                    report.dimensions = Some((
                        u32::from_be_bytes(
                            payload[..4].try_into().map_err(|_| ScanError::Corrupt)?,
                        ),
                        u32::from_be_bytes(
                            payload[4..8].try_into().map_err(|_| ScanError::Corrupt)?,
                        ),
                    ));
                    if options.dimensions_only {
                        return Ok(());
                    }
                }
                None => push_warning(
                    report,
                    "image.png.crc-invalid",
                    ImageIssueSeverity::Warning,
                    "an IHDR checksum is invalid",
                    Some(offset),
                ),
            }
        } else if &kind == b"eXIf" {
            if length > EXIF_BLOCK_READ_LIMIT {
                exif::record_skipped_block(report, "png.exif", offset, length as u64);
            } else {
                match verified_payload(reader, offset, length, &kind)? {
                    Some(payload) => {
                        exif::decode_block(report, "png.exif", offset, payload);
                    }
                    None => push_warning(
                        report,
                        "image.png.crc-invalid",
                        ImageIssueSeverity::Warning,
                        "an eXIf checksum is invalid",
                        Some(offset),
                    ),
                }
            }
        } else if &kind == b"IEND" {
            if length != 0 {
                return Err(ScanError::Corrupt);
            }
            set_declared_end(report, end);
            return Ok(());
        }
        offset = end;
        index += 1;
    }
    push_finding(
        report,
        "image.segment-limit",
        ImageIssueSeverity::Warning,
        Some(offset),
        Some("the PNG chunk limit was reached".to_string()),
    );
    Err(ScanError::Limit)
}

fn verified_payload(
    reader: &mut ScanReader<'_>,
    offset: u64,
    length: usize,
    kind: &[u8; 4],
) -> Result<Option<Vec<u8>>, ScanError> {
    let payload = reader.read(offset + 8, length)?;
    let crc = reader.read(offset + 8 + length as u64, 4)?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(kind);
    hasher.update(&payload);
    if hasher.finalize().to_be_bytes() != crc.as_slice() {
        return Ok(None);
    }
    Ok(Some(payload))
}
