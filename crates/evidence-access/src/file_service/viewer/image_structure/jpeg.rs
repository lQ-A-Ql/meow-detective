//! Native JPEG segment ranges and frame-header semantics behind one shared facade.

use image_inspection::{
    jpeg_codec::{frame_dimensions, Marker},
    jpeg_segments::{jpeg_marker, ScannedSegment, SegmentScanner},
};
use std::io::Cursor;

use super::exif;
use super::limits::SEGMENT_LIMIT;
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_segment, set_declared_end, ImageAnalysisReport, ImageAnalyzeOptions,
};

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let mut scanner = SegmentScanner::new(jpeg_marker::EOI);
    let mut offset = 0;
    let mut count = 0;
    let mut previous_end = 0;
    while offset < reader.size() {
        let bytes = if options.dimensions_only {
            reader.read(offset, (reader.size() - offset).min(4096) as usize)?
        } else {
            let Some(bytes) = reader.read_scan(offset, 4096)? else {
                return Err(ScanError::Limit);
            };
            bytes
        };
        scanner.on_bytes(&bytes);
        let failed = scanner.had_error();
        for segment in scanner.segments.drain(..) {
            count += 1;
            if count > SEGMENT_LIMIT {
                return Err(ScanError::Limit);
            }
            let start = u64::from(segment.offset);
            let end = start + 2 + u64::from(segment.parameter_length);
            if end > reader.size() {
                mark_truncated(report, "image.truncated", start);
                return Ok(());
            }
            let marker = Marker::from_u8(segment.marker).ok_or(ScanError::Corrupt)?;
            if start < previous_end || (marker == Marker::SOI && count != 1) {
                return Err(ScanError::Corrupt);
            }
            inspect_segment(reader, report, options, &segment, marker)?;
            push_segment(
                report,
                segment_kind(marker, segment.marker),
                start,
                end - start,
                None,
            );
            if marker == Marker::EOI {
                set_declared_end(report, end);
                return Ok(());
            }
            if options.dimensions_only && report.dimensions.is_some() {
                return Ok(());
            }
            previous_end = end;
        }
        if failed {
            return Err(ScanError::Corrupt);
        }
        offset += bytes.len() as u64;
    }
    mark_truncated(report, "image.jpeg.eoi-missing", offset);
    Ok(())
}

fn inspect_segment(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
    segment: &ScannedSegment,
    marker: Marker,
) -> Result<(), ScanError> {
    let parameters = u64::from(segment.offset) + 2;
    let length = usize::from(segment.parameter_length);
    match marker {
        Marker::SOF(_) => {
            let frame = reader.read(parameters, length)?;
            let (width, height) = frame_dimensions(&mut Cursor::new(frame), segment.marker)
                .map_err(|_| ScanError::Corrupt)?;
            if report.dimensions.is_none() {
                report.dimensions = Some((u32::from(width), u32::from(height)));
            }
        }
        Marker::APP(1) if !options.dimensions_only => {
            let payload = reader.read(
                parameters + 2,
                length.checked_sub(2).ok_or(ScanError::Corrupt)?,
            )?;
            if let Some(bytes) = payload.strip_prefix(b"Exif\0\0") {
                exif::decode_block(report, "jpeg.app1", parameters + 8, bytes.to_vec());
            }
        }
        _ => {}
    }
    Ok(())
}

fn segment_kind(marker: Marker, byte: u8) -> String {
    match marker {
        Marker::SOI => "jpeg.soi".into(),
        Marker::EOI => "jpeg.eoi".into(),
        Marker::SOS => "jpeg.sos".into(),
        Marker::SOF(_) => "jpeg.sof".into(),
        Marker::DHT => "jpeg.dht".into(),
        Marker::DQT => "jpeg.dqt".into(),
        Marker::DRI => "jpeg.dri".into(),
        Marker::COM => "jpeg.com".into(),
        Marker::APP(index) => format!("jpeg.app{index}"),
        Marker::RST(index) => format!("jpeg.rst{index}"),
        _ => format!("jpeg.marker.{byte:02x}"),
    }
}
