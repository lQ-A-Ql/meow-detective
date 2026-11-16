//! TIFF IFD traversal with cycle, offset, and referenced-data bounds.

use std::collections::HashSet;

use super::exif;
use super::limits::{IFD_ENTRY_LIMIT, IFD_LIMIT};
use super::source::{ScanError, ScanReader};
use super::{
    mark_truncated, push_warning, read_u16, read_u32, set_declared_end, ImageAnalysisReport,
    ImageAnalyzeOptions, ImageIssueSeverity,
};

pub(super) fn scan(
    reader: &mut ScanReader<'_>,
    report: &mut ImageAnalysisReport,
    options: &ImageAnalyzeOptions,
) -> Result<(), ScanError> {
    let size = reader.size();
    if size < 8 {
        return Err(ScanError::Corrupt);
    }
    let window_length = size.min(options.random_read_limit as u64) as usize;
    let window = reader.read(0, window_length)?;
    let little = match window.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return Err(ScanError::Corrupt),
    };
    if window.len() < 8 {
        mark_truncated(report, "image.truncated", window.len() as u64);
        return Ok(());
    }
    if read_u16(&window, 2, little) != Some(42) {
        return Err(ScanError::Corrupt);
    }
    let ifd0 = u64::from(read_u32(&window, 4, little).ok_or(ScanError::Corrupt)?);
    if window.len() < size as usize {
        push_warning(
            report,
            "image.tiff.window-limited",
            ImageIssueSeverity::Info,
            "the TIFF directory was analysed from a bounded prefix",
            Some(window.len() as u64),
        );
    }
    let mut walk = TiffWalk {
        window: &window,
        size,
        little,
        entries_total: 0,
        ifd_count: 0,
        max_end: 8,
        pending: vec![ifd0],
        visited: HashSet::new(),
        width: None,
        height: None,
    };
    loop {
        match walk.step(report)? {
            WalkStep::Continue => {
                if options.dimensions_only && report.dimensions.is_some() {
                    break;
                }
            }
            WalkStep::Finished => break,
            WalkStep::Truncated => return Ok(()),
        }
    }
    let max_end = walk.max_end;
    drop(walk);
    if max_end > size {
        mark_truncated(report, "image.truncated", size);
        return Ok(());
    }
    set_declared_end(report, max_end);
    if options.dimensions_only {
        return Ok(());
    }
    exif::decode_block(report, "tiff.ifd0", 0, window);
    Ok(())
}

enum WalkStep {
    Continue,
    Finished,
    Truncated,
}

struct TiffWalk<'a> {
    window: &'a [u8],
    size: u64,
    little: bool,
    entries_total: usize,
    ifd_count: usize,
    max_end: u64,
    pending: Vec<u64>,
    visited: HashSet<u64>,
    width: Option<u32>,
    height: Option<u32>,
}

impl TiffWalk<'_> {
    fn step(&mut self, report: &mut ImageAnalysisReport) -> Result<WalkStep, ScanError> {
        let Some(offset) = self.pending.pop() else {
            return Ok(WalkStep::Finished);
        };
        if offset == 0 {
            return Ok(WalkStep::Continue);
        }
        if !self.visited.insert(offset) {
            return Err(ScanError::Corrupt);
        }
        self.ifd_count += 1;
        if self.ifd_count > IFD_LIMIT {
            return Err(ScanError::Limit);
        }
        let offset_usize = usize::try_from(offset).map_err(|_| ScanError::Corrupt)?;
        if offset_usize + 2 > self.window.len() {
            if offset < self.size {
                push_warning(
                    report,
                    "image.tiff.ifd-outside-window",
                    ImageIssueSeverity::Warning,
                    "a TIFF IFD lies outside the analysed prefix",
                    Some(offset),
                );
                return Ok(WalkStep::Finished);
            }
            return Err(ScanError::Corrupt);
        }
        let count = usize::from(
            read_u16(self.window, offset_usize, self.little).ok_or(ScanError::Corrupt)?,
        );
        self.entries_total += count;
        if self.entries_total > IFD_ENTRY_LIMIT {
            return Err(ScanError::Limit);
        }
        let table_end = offset_usize
            .checked_add(2 + count * 12 + 4)
            .ok_or(ScanError::Corrupt)?;
        if table_end > self.window.len() {
            if (table_end as u64) <= self.size {
                push_warning(
                    report,
                    "image.tiff.ifd-outside-window",
                    ImageIssueSeverity::Warning,
                    "a TIFF IFD lies outside the analysed prefix",
                    Some(offset),
                );
                return Ok(WalkStep::Finished);
            }
            return Err(ScanError::Corrupt);
        }
        self.max_end = self.max_end.max(table_end as u64);
        for index in 0..count {
            let step = self.visit_entry(report, offset_usize, index)?;
            if matches!(step, WalkStep::Truncated) {
                return Ok(step);
            }
        }
        let next = read_u32(self.window, table_end - 4, self.little).ok_or(ScanError::Corrupt)?;
        if next != 0 {
            self.pending.push(u64::from(next));
        }
        Ok(WalkStep::Continue)
    }

    fn visit_entry(
        &mut self,
        report: &mut ImageAnalysisReport,
        offset_usize: usize,
        index: usize,
    ) -> Result<WalkStep, ScanError> {
        let position = offset_usize + 2 + index * 12;
        let tag = read_u16(self.window, position, self.little).ok_or(ScanError::Corrupt)?;
        let kind = read_u16(self.window, position + 2, self.little).ok_or(ScanError::Corrupt)?;
        let value_count =
            u64::from(read_u32(self.window, position + 4, self.little).ok_or(ScanError::Corrupt)?);
        let width_bytes = match kind {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 => 4,
            5 | 10 | 12 => 8,
            _ => 0,
        };
        let value_size = width_bytes * value_count;
        let value_field =
            u64::from(read_u32(self.window, position + 8, self.little).ok_or(ScanError::Corrupt)?);
        if value_size > 4 {
            let end = value_field
                .checked_add(value_size)
                .ok_or(ScanError::Corrupt)?;
            if end > self.size {
                mark_truncated(report, "image.truncated", position as u64);
                return Ok(WalkStep::Truncated);
            }
            self.max_end = self.max_end.max(end);
        }
        match tag {
            0x0100 => self.width = inline_uint(self.window, position, self.little, value_count),
            0x0101 => self.height = inline_uint(self.window, position, self.little, value_count),
            0x8769 | 0x8825 | 0xa005 => self.pending.push(value_field),
            _ => {}
        }
        if let (Some(width), Some(height)) = (self.width, self.height) {
            report.dimensions = Some((width, height));
        }
        Ok(WalkStep::Continue)
    }
}

fn inline_uint(window: &[u8], position: usize, little: bool, count: u64) -> Option<u32> {
    if count != 1 {
        return None;
    }
    let kind = read_u16(window, position + 2, little)?;
    let raw = window.get(position + 8..position + 12)?;
    match kind {
        3 => read_u16(window, position + 8, little).map(u32::from),
        4 => read_u32(window, position + 8, little),
        _ => {
            let _ = raw;
            None
        }
    }
}
