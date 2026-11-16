//! JPEG marker traversal with marker-aware entropy scanning.

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
    let mut offset = 2u64;
    let mut segment_count = 0usize;
    loop {
        if segment_count >= SEGMENT_LIMIT {
            push_finding(
                report,
                "image.segment-limit",
                ImageIssueSeverity::Warning,
                Some(offset),
                Some("the JPEG segment limit was reached".to_string()),
            );
            return Err(ScanError::Limit);
        }
        if offset + 2 > size {
            mark_truncated(report, "image.truncated", offset);
            return Ok(());
        }
        let mut marker = reader.read(offset, 2)?;
        if marker[0] != 0xff {
            return Err(ScanError::Corrupt);
        }
        while marker[1] == 0xff {
            offset += 1;
            if offset + 2 > size {
                mark_truncated(report, "image.truncated", offset);
                return Ok(());
            }
            marker = reader.read(offset, 2)?;
        }
        if marker[1] == 0x00 {
            return Err(ScanError::Corrupt);
        }
        let marker_start = offset;
        let marker_type = marker[1];
        offset += 2;
        segment_count += 1;
        match marker_type {
            0xd8 | 0x01 | 0xd0..=0xd7 => {
                push_segment(
                    report,
                    format!("jpeg.marker.{marker_type:02x}"),
                    marker_start,
                    2,
                    None,
                );
            }
            0xd9 => {
                push_segment(report, "jpeg.eoi".to_string(), marker_start, 2, None);
                set_declared_end(report, offset);
                return Ok(());
            }
            0xda => match scan_sos(reader, report, options, marker_start, offset, size)? {
                Some(next) => offset = next,
                None => return Ok(()),
            },
            _ => {
                match scan_regular_segment(
                    reader,
                    report,
                    options,
                    marker_start,
                    marker_type,
                    offset,
                    size,
                )? {
                    Some(end) => offset = end,
                    None => return Ok(()),
                }
            }
        }
    }
}

fn scan_sos(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
    marker_start: u64,
    offset: u64,
    size: u64,
) -> Result<Option<u64>, ScanError> {
    let length = segment_length(reader, offset, size)?;
    push_segment(
        report,
        "jpeg.sos".to_string(),
        marker_start,
        2 + u64::from(length),
        None,
    );
    if options.dimensions_only {
        return Ok(None);
    }
    let entropy_start = offset + u64::from(length);
    match scan_entropy(reader, entropy_start, options.scan_limit)? {
        EntropyStop::Eoi(end) => {
            set_declared_end(report, end);
            Ok(None)
        }
        EntropyStop::NextMarker(next) => Ok(Some(next)),
        EntropyStop::Capped => {
            push_finding(
                report,
                "image.jpeg.scan-capped",
                ImageIssueSeverity::Warning,
                Some(entropy_start),
                Some("the JPEG scan limit was reached before EOI".to_string()),
            );
            Ok(None)
        }
        EntropyStop::Eof => {
            mark_truncated(report, "image.jpeg.eoi-missing", entropy_start);
            Ok(None)
        }
    }
}

fn scan_regular_segment(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
    marker_start: u64,
    marker_type: u8,
    offset: u64,
    size: u64,
) -> Result<Option<u64>, ScanError> {
    let length = segment_length(reader, offset, size)?;
    let end = offset + u64::from(length);
    push_segment(
        report,
        segment_kind(marker_type),
        marker_start,
        2 + u64::from(length),
        None,
    );
    if is_sof(marker_type) && length >= 7 {
        let frame = reader.read(offset + 2, 5)?;
        let height = u32::from(u16::from_be_bytes([frame[1], frame[2]]));
        let width = u32::from(u16::from_be_bytes([frame[3], frame[4]]));
        if report.dimensions.is_none() {
            report.dimensions = Some((width, height));
        }
        if options.dimensions_only {
            return Ok(None);
        }
    } else if marker_type == 0xe1 {
        collect_exif(reader, report, offset, length)?;
    }
    Ok(Some(end))
}

fn segment_length(reader: &mut ScanReader<'_>, offset: u64, size: u64) -> Result<u16, ScanError> {
    let bytes = reader.read(offset, 2)?;
    let length = u16::from_be_bytes([bytes[0], bytes[1]]);
    if length < 2 || offset + u64::from(length) > size {
        return Err(ScanError::Corrupt);
    }
    Ok(length)
}

fn collect_exif(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    offset: u64,
    length: u16,
) -> Result<(), ScanError> {
    let payload_length = usize::from(length) - 2;
    if payload_length > EXIF_BLOCK_READ_LIMIT {
        push_warning(
            report,
            "image.exif.block-too-large",
            ImageIssueSeverity::Warning,
            "an EXIF block exceeded the decode limit",
            Some(offset),
        );
        return Ok(());
    }
    let payload = reader.read(offset + 2, payload_length)?;
    if let Some(bytes) = payload.strip_prefix(b"Exif\0\0") {
        exif::decode_block(report, "jpeg.app1", offset + 2, bytes.to_vec());
    }
    Ok(())
}

fn is_sof(marker: u8) -> bool {
    matches!(
        marker,
        0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf
    )
}

fn segment_kind(marker: u8) -> String {
    match marker {
        0xc4 => "jpeg.dht".to_string(),
        0xdb => "jpeg.dqt".to_string(),
        0xdd => "jpeg.dri".to_string(),
        0xfe => "jpeg.com".to_string(),
        0xe0..=0xef => format!("jpeg.app{}", marker - 0xe0),
        0xc0..=0xcf if is_sof(marker) => "jpeg.sof".to_string(),
        _ => format!("jpeg.marker.{marker:02x}"),
    }
}

enum EntropyStop {
    Eoi(u64),
    NextMarker(u64),
    Capped,
    Eof,
}

fn scan_entropy(
    reader: &mut ScanReader<'_>,
    start: u64,
    scan_limit: usize,
) -> Result<EntropyStop, ScanError> {
    let file_size = reader.size();
    let Some(data) = reader.read_scan(start, scan_limit)? else {
        return Ok(EntropyStop::Capped);
    };
    let mut index = 0usize;
    while index + 1 < data.len() {
        if data[index] != 0xff {
            index += 1;
            continue;
        }
        match data[index + 1] {
            0x00 | 0xd0..=0xd7 => index += 2,
            0xff => index += 1,
            0xd9 => return Ok(EntropyStop::Eoi(start + index as u64 + 2)),
            0xda => return Ok(EntropyStop::NextMarker(start + index as u64)),
            _ => return Ok(EntropyStop::NextMarker(start + index as u64)),
        }
    }
    if start + data.len() as u64 >= file_size {
        Ok(EntropyStop::Eof)
    } else {
        Ok(EntropyStop::Capped)
    }
}
