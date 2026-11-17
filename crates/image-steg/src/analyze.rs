use crate::decode::{bmp, png, DecodeError, RowSource};
use crate::limits::StegLimits;
use crate::scan::ScanSink;
use crate::types::{StegFormat, StegReport, StegStatus};

/// Runs triage on a complete in-memory image. Never panics on malformed input;
/// unsupported containers and decode failures are reported as statuses.
pub fn analyze(bytes: &[u8], format: StegFormat, limits: &StegLimits) -> StegReport {
    if bytes.len() as u64 > limits.max_input_bytes {
        return StegReport::failed(StegStatus::Oversized, Some(format), None, "limit.bytes");
    }
    match format {
        StegFormat::Png => run(png::open(bytes, limits), format, limits),
        StegFormat::Bmp => run(bmp::open(bytes, limits), format, limits),
    }
}

fn run<S: RowSource>(
    source: Result<S, DecodeError>,
    format: StegFormat,
    limits: &StegLimits,
) -> StegReport {
    let mut source = match source {
        Ok(source) => source,
        Err(error) => {
            return StegReport::failed(
                error.status,
                Some(format),
                error.width.zip(error.height),
                error.reason,
            )
        }
    };
    let header = source.header();
    let dims = (header.width, header.height);
    let mut sink = ScanSink::new(header, limits);
    loop {
        match source.next_row(&mut sink) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => {
                return StegReport::failed(
                    error.status,
                    Some(format),
                    error.width.zip(error.height).or(Some(dims)),
                    error.reason,
                )
            }
        }
    }
    if sink.rows_seen() != header.height {
        return StegReport::failed(
            StegStatus::Corrupt,
            Some(format),
            Some(dims),
            "image.truncated",
        );
    }
    let results = sink.finish();
    StegReport {
        status: StegStatus::Completed,
        format: Some(format),
        width: Some(header.width),
        height: Some(header.height),
        planes: results.planes,
        global: results.global,
        dominant_plane: results.dominant,
        blocks: results.blocks,
        blocks_scored: results.blocks_scored,
        blocks_total: results.blocks_total,
        warnings: results.warnings,
        reason: None,
    }
}
