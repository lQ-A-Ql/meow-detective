//! TIFF structure and raster extents obtained from the shared native EXIF parser.

use exif::{Exif, In, Tag};

use super::exif as metadata;
use super::limits::EXIF_BLOCK_READ_LIMIT;
use super::source::{ScanError, ScanReader};
use super::{mark_truncated, set_declared_end, ImageAnalysisReport, ImageAnalyzeOptions};

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let size = reader.size();
    let length = size
        .min(options.random_read_limit.saturating_sub(16) as u64)
        .min(EXIF_BLOCK_READ_LIMIT as u64) as usize;
    let window = reader.read(0, length)?;
    let parsed = metadata::read_tiff(window).map_err(|error| match error {
        exif::Error::TooBig(_) => ScanError::Limit,
        _ if (length as u64) < size => ScanError::Limit,
        _ => ScanError::Corrupt,
    })?;
    report.dimensions =
        dimension(&parsed, Tag::ImageWidth).zip(dimension(&parsed, Tag::ImageLength));
    if options.dimensions_only {
        return Ok(());
    }
    let end = raster_end(&parsed)?;
    if end > size {
        mark_truncated(report, "image.truncated", size);
        return Ok(());
    }
    set_declared_end(report, end);
    metadata::record_parsed_block(report, "tiff.ifd0", 0, length as u64, Ok(parsed));
    Ok(())
}

fn dimension(parsed: &Exif, tag: Tag) -> Option<u32> {
    let field = parsed.get_field(tag, In::PRIMARY)?;
    let mut values = field.value.iter_uint()?;
    if values.len() != 1 {
        return None;
    }
    values.next().filter(|value| *value != 0)
}

fn raster_end(parsed: &Exif) -> Result<u64, ScanError> {
    let mut end = parsed.storage_end() as u64;
    for ifd in 0..8 {
        for (offset_tag, length_tag) in [
            (Tag::StripOffsets, Tag::StripByteCounts),
            (Tag::TileOffsets, Tag::TileByteCounts),
            (Tag::JPEGInterchangeFormat, Tag::JPEGInterchangeFormatLength),
        ] {
            let offsets = parsed.get_field(offset_tag, In(ifd));
            let lengths = parsed.get_field(length_tag, In(ifd));
            let (offsets, lengths) = match (offsets, lengths) {
                (None, None) => continue,
                (Some(offsets), Some(lengths)) => (offsets, lengths),
                _ => return Err(ScanError::Corrupt),
            };
            let offsets = offsets.value.iter_uint().ok_or(ScanError::Corrupt)?;
            let lengths = lengths.value.iter_uint().ok_or(ScanError::Corrupt)?;
            if offsets.len() != lengths.len() || offsets.len() == 0 {
                return Err(ScanError::Corrupt);
            }
            for (offset, length) in offsets.zip(lengths) {
                end = end.max(u64::from(offset) + u64::from(length));
            }
        }
    }
    Ok(end)
}
