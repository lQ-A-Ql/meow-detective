//! Bounded structural inspection through the shared native PNG decoder.

use super::exif;
use super::limits::{EXIF_BLOCK_READ_LIMIT, SEGMENT_LIMIT};
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_finding, push_segment, push_warning, set_declared_end,
    ImageAnalysisReport, ImageAnalyzeOptions, ImageIssueSeverity,
};
use image_inspection::png_codec::{chunk, Decoded, StreamingDecoder};

struct ChunkRange {
    kind: chunk::ChunkType,
    offset: u64,
    payload_length: u32,
}

struct Walker {
    decoder: StreamingDecoder,
    position: u64,
    chunks: usize,
    current: Option<ChunkRange>,
}

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let mut decoder = StreamingDecoder::new();
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let mut walker = Walker {
        decoder,
        position: 0,
        chunks: 0,
        current: None,
    };
    let mut offset = 0;
    loop {
        let bytes = if options.dimensions_only {
            reader.read(0, reader.size().min(33) as usize)?
        } else {
            let Some(bytes) = reader.read_scan(offset, 64 * 1024)? else {
                break;
            };
            bytes
        };
        let mut consumed = 0;
        while consumed < bytes.len() {
            let (count, event) =
                walker
                    .decoder
                    .update(&bytes[consumed..], None)
                    .map_err(|error| match error {
                        image_inspection::png_codec::DecodingError::LimitsExceeded => {
                            ScanError::Limit
                        }
                        _ => ScanError::Corrupt,
                    })?;
            if count == 0 && matches!(event, Decoded::Nothing) {
                return Err(ScanError::Corrupt);
            }
            consumed += count;
            walker.position = walker
                .position
                .checked_add(count as u64)
                .ok_or(ScanError::Corrupt)?;
            if walker.process(event, reader, report, options)? {
                return Ok(());
            }
        }
        if options.dimensions_only {
            return Err(ScanError::Corrupt);
        }
        offset += bytes.len() as u64;
    }
    if offset < reader.size() {
        return Err(ScanError::Limit);
    }
    mark_truncated(report, "image.png.missing-iend", walker.position);
    Ok(())
}

impl Walker {
    fn process(
        &mut self,
        event: Decoded,
        reader: &mut ScanReader<'_>,
        report: &mut ImageAnalysisReport,
        options: &ImageAnalyzeOptions,
    ) -> Result<bool, ScanError> {
        if matches!(&event, Decoded::BadAncillaryChunk(_)) {
            push_finding(
                report,
                "image.png.chunk-invalid",
                ImageIssueSeverity::Warning,
                self.current.as_ref().map(|range| range.offset),
                Some("an ancillary PNG chunk is invalid or duplicated".into()),
            );
        }
        match event {
            Decoded::ChunkBegin(length, kind) => {
                self.chunks += 1;
                let offset = self.position.checked_sub(8).ok_or(ScanError::Corrupt)?;
                if self.chunks > SEGMENT_LIMIT {
                    push_finding(
                        report,
                        "image.segment-limit",
                        ImageIssueSeverity::Warning,
                        Some(offset),
                        Some("the PNG chunk limit was reached".into()),
                    );
                    return Err(ScanError::Limit);
                }
                let end = offset
                    .checked_add(u64::from(length) + 12)
                    .ok_or(ScanError::Corrupt)?;
                if end > reader.size() {
                    mark_truncated(report, "image.truncated", offset);
                    return Ok(true);
                }
                push_segment(
                    report,
                    format!("png.{}", String::from_utf8_lossy(&kind.0)),
                    offset,
                    end - offset,
                    None,
                );
                self.current = Some(ChunkRange {
                    kind,
                    offset,
                    payload_length: length,
                });
            }
            Decoded::ChunkComplete(kind) | Decoded::BadAncillaryChunk(kind) => {
                if kind == chunk::IHDR {
                    let info = self.decoder.info().ok_or(ScanError::Corrupt)?;
                    report.dimensions = Some((info.width, info.height));
                    if options.dimensions_only {
                        return Ok(true);
                    }
                }
                if kind == chunk::eXIf {
                    let range = self.current.as_ref().ok_or(ScanError::Corrupt)?;
                    record_exif(reader, report, range)?;
                }
                if kind == chunk::IEND {
                    set_declared_end(report, self.position);
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(false)
    }
}

fn record_exif(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    range: &ChunkRange,
) -> Result<(), ScanError> {
    if range.payload_length as usize > EXIF_BLOCK_READ_LIMIT {
        exif::record_skipped_block(
            report,
            "png.exif",
            range.offset,
            u64::from(range.payload_length),
        );
        return Ok(());
    }
    let length = range.payload_length as usize;
    let payload = reader.read(range.offset + 8, length)?;
    let crc = reader.read(range.offset + 8 + length as u64, 4)?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&range.kind.0);
    hasher.update(&payload);
    if hasher.finalize().to_be_bytes() == crc.as_slice() {
        exif::decode_block(report, "png.exif", range.offset, payload);
    } else {
        push_warning(
            report,
            "image.png.crc-invalid",
            ImageIssueSeverity::Warning,
            "an eXIf checksum is invalid",
            Some(range.offset),
        );
    }
    Ok(())
}
