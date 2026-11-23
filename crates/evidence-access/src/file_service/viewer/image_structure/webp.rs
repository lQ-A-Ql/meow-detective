//! WebP source ranges and metadata through native container/header readers.

use image_inspection::webp_codec::inspection::{
    read_chunk_header, read_container_header, read_dimensions, WebPRiffChunk,
};
use std::io::Cursor;

use super::exif;
use super::limits::{EXIF_BLOCK_READ_LIMIT, SEGMENT_LIMIT};
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_segment, set_declared_end, ImageAnalysisReport, ImageAnalyzeOptions,
};

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let header = reader.read(0, 12)?;
    let declared_end =
        read_container_header(Cursor::new(header)).map_err(|_| ScanError::Corrupt)?;
    if declared_end > reader.size() {
        mark_truncated(report, "image.truncated", reader.size());
        return Ok(());
    }
    let mut offset = 12;
    let mut index = 0;
    while offset < declared_end {
        if index >= SEGMENT_LIMIT {
            return Err(ScanError::Limit);
        }
        if declared_end - offset < 8 {
            return Err(ScanError::Corrupt);
        }
        let header = reader.read(offset, 8)?;
        let (kind, length, padded_length) =
            read_chunk_header(Cursor::new(header)).map_err(|_| ScanError::Corrupt)?;
        let next = offset + 8 + padded_length;
        if next > declared_end {
            return Err(ScanError::Corrupt);
        }
        push_segment(
            report,
            format!("webp.{}", String::from_utf8_lossy(&kind.to_fourcc())),
            offset,
            next - offset,
            None,
        );
        inspect_chunk(reader, report, options, kind, offset + 8, length)?;
        if options.dimensions_only && report.dimensions.is_some() {
            return Ok(());
        }
        offset = next;
        index += 1;
    }
    set_declared_end(report, declared_end);
    Ok(())
}

fn inspect_chunk(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
    kind: WebPRiffChunk,
    payload_offset: u64,
    length: u64,
) -> Result<(), ScanError> {
    match kind {
        WebPRiffChunk::EXIF if !options.dimensions_only => {
            if length > EXIF_BLOCK_READ_LIMIT as u64 {
                exif::record_skipped_block(report, "webp.exif", payload_offset, length);
            } else {
                let mut payload = reader.read(payload_offset, length as usize)?;
                let prefix = if payload.starts_with(b"Exif\0\0") {
                    6
                } else {
                    0
                };
                if prefix != 0 {
                    payload.drain(..prefix);
                }
                exif::decode_block(report, "webp.exif", payload_offset + prefix as u64, payload);
            }
        }
        WebPRiffChunk::VP8 | WebPRiffChunk::VP8L | WebPRiffChunk::VP8X => {
            let payload = reader.read(payload_offset, length.min(10) as usize)?;
            let dimensions =
                read_dimensions(Cursor::new(payload), kind).map_err(|_| ScanError::Corrupt)?;
            if report.dimensions.is_none() || kind == WebPRiffChunk::VP8X {
                report.dimensions = Some(dimensions);
            }
        }
        _ => {}
    }
    Ok(())
}
