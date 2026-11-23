//! Bounded GIF structure inspection through native decoder events.

use image_inspection::gif_codec::{
    streaming_decoder::{Block, Decoded, OutputBuffer, StreamingDecoder},
    DecodeOptions, DecodingError,
};

use super::limits::{GIF_SUB_BLOCK_LIMIT, SEGMENT_LIMIT};
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_segment, set_declared_end, ImageAnalysisReport, ImageAnalyzeOptions,
};

struct BlockRange {
    kind: String,
    offset: u64,
}

struct Walker {
    decoder: StreamingDecoder,
    position: u64,
    next_offset: u64,
    blocks: usize,
    sub_blocks: usize,
    current: Option<BlockRange>,
}

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let mut settings = DecodeOptions::new();
    settings.skip_frame_decoding(true);
    settings.check_frame_consistency(true);
    settings.allow_unknown_blocks(true);
    let mut walker = Walker {
        decoder: StreamingDecoder::with_options(&settings),
        position: 0,
        next_offset: 0,
        blocks: 0,
        sub_blocks: 0,
        current: None,
    };
    let mut offset = 0;
    loop {
        let bytes = if options.dimensions_only {
            reader.read(0, reader.size().min(13) as usize)?
        } else {
            let Some(bytes) = reader.read_scan(offset, 64 * 1024)? else {
                break;
            };
            bytes
        };
        let mut consumed = 0;
        while consumed < bytes.len() {
            let (count, event) = walker
                .decoder
                .update(&bytes[consumed..], &mut OutputBuffer::None)
                .map_err(|error| match error {
                    DecodingError::OutOfMemory | DecodingError::MemoryLimit => ScanError::Limit,
                    _ => ScanError::Corrupt,
                })?;
            if count == 0 && matches!(event, Decoded::Nothing) {
                return Err(ScanError::Corrupt);
            }
            consumed += count;
            walker.position += count as u64;
            if walker.process(event, report, options)? {
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
    mark_truncated(report, "image.gif.missing-trailer", walker.position);
    Ok(())
}

impl Walker {
    fn process(
        &mut self,
        event: Decoded,
        report: &mut ImageAnalysisReport,
        options: &ImageAnalyzeOptions,
    ) -> Result<bool, ScanError> {
        match event {
            Decoded::BackgroundColor(_) => {
                let width = self.decoder.width();
                let height = self.decoder.height();
                if width == 0 || height == 0 {
                    return Err(ScanError::Corrupt);
                }
                report.dimensions = Some((u32::from(width), u32::from(height)));
                if options.dimensions_only {
                    return Ok(true);
                }
            }
            Decoded::GlobalPalette(_) => {
                let end = self.position.checked_sub(1).ok_or(ScanError::Corrupt)?;
                push_segment(report, "gif.header".into(), 0, end, None);
                self.next_offset = end;
            }
            Decoded::BlockStart(Block::Trailer) => {
                let offset = self.position.checked_sub(1).ok_or(ScanError::Corrupt)?;
                if self.current.is_some() || offset != self.next_offset {
                    return Err(ScanError::Corrupt);
                }
                push_segment(report, "gif.trailer".into(), offset, 1, None);
                set_declared_end(report, self.position);
                return Ok(true);
            }
            Decoded::BlockStart(kind) => self.begin_block(kind)?,
            Decoded::SubBlock { ext, is_last } => {
                self.sub_blocks += 1;
                if self.sub_blocks > GIF_SUB_BLOCK_LIMIT {
                    return Err(ScanError::Limit);
                }
                let block = self.current.as_mut().ok_or(ScanError::Corrupt)?;
                block.kind = format!("gif.extension.{:02x}", ext.0);
                if is_last {
                    self.finish_block(report)?;
                }
            }
            Decoded::FrameMetadata(_) => {
                let frame = self.decoder.current_frame();
                if frame.width == 0 || frame.height == 0 {
                    return Err(ScanError::Corrupt);
                }
            }
            Decoded::DataEnd => self.finish_block(report)?,
            _ => {}
        }
        Ok(false)
    }

    fn begin_block(&mut self, kind: Block) -> Result<(), ScanError> {
        self.blocks += 1;
        if self.blocks > SEGMENT_LIMIT {
            return Err(ScanError::Limit);
        }
        let (kind, consumed) = match kind {
            Block::Image => ("gif.image", 1),
            Block::Extension => ("gif.extension", 3),
            Block::Trailer => return Err(ScanError::Corrupt),
        };
        let offset = self
            .position
            .checked_sub(consumed)
            .ok_or(ScanError::Corrupt)?;
        if self.current.is_some() || offset != self.next_offset {
            return Err(ScanError::Corrupt);
        }
        self.current = Some(BlockRange {
            kind: kind.into(),
            offset,
        });
        self.sub_blocks = 0;
        Ok(())
    }

    fn finish_block(&mut self, report: &mut ImageAnalysisReport) -> Result<(), ScanError> {
        let block = self.current.take().ok_or(ScanError::Corrupt)?;
        push_segment(
            report,
            block.kind,
            block.offset,
            self.position - block.offset,
            None,
        );
        self.next_offset = self.position;
        Ok(())
    }
}
