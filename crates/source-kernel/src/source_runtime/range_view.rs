use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use domain::structure::ByteRangeSet;
use evidence_core::{
    reader::{EvidenceReadMetrics, ReaderResourceHold},
    EvidenceReader, ReaderInfo,
};

pub struct VerifiedView<'cancel> {
    reader: Box<dyn EvidenceReader>,
    ranges: ByteRangeSet,
    range_index: usize,
    range_offset: u64,
    info: ReaderInfo,
    cancel: &'cancel AtomicBool,
    deadline_unix_ms: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum RangeReadError {
    #[error("range read cancelled")]
    Cancelled,
}

impl<'cancel> VerifiedView<'cancel> {
    pub fn new(
        mut reader: Box<dyn EvidenceReader>,
        ranges: ByteRangeSet,
        cancel: &'cancel AtomicBool,
        deadline_unix_ms: Option<u64>,
    ) -> io::Result<Self> {
        ranges.validate().map_err(io::Error::other)?;
        if reader.info().size != ranges.target_size() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "range target size differs from reader",
            ));
        }
        check_active(cancel, deadline_unix_ms)?;
        if let Some(range) = ranges.ranges().first() {
            reader.seek(SeekFrom::Start(range.start()))?;
        }
        let mut info = reader.info().clone();
        info.size = ranges.covered_bytes();
        Ok(Self {
            reader,
            ranges,
            range_index: 0,
            range_offset: 0,
            info,
            cancel,
            deadline_unix_ms,
        })
    }

    fn check_active(&self) -> io::Result<()> {
        check_active(self.cancel, self.deadline_unix_ms)?;
        self.reader.validate_read_scope()
    }
    fn current_range(&self) -> Option<domain::structure::ByteRange> {
        self.ranges.ranges().get(self.range_index).copied()
    }
    fn virtual_position(&self) -> u64 {
        self.ranges.ranges()[..self.range_index]
            .iter()
            .map(|range| range.length())
            .sum::<u64>()
            + self.range_offset
    }
    fn seek_virtual(&mut self, position: u64) -> io::Result<u64> {
        if position > self.info.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "range seek exceeds covered bytes",
            ));
        }
        if position == self.info.size {
            self.range_index = self.ranges.ranges().len();
            self.range_offset = 0;
            return Ok(position);
        }
        let mut remaining = position;
        for (index, range) in self.ranges.ranges().iter().enumerate() {
            if remaining < range.length() {
                self.reader
                    .seek(SeekFrom::Start(range.start() + remaining))?;
                self.range_index = index;
                self.range_offset = remaining;
                return Ok(position);
            }
            remaining -= range.length();
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "range seek is outside covered bytes",
        ))
    }
}

impl Read for VerifiedView<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.check_active()?;
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let Some(range) = self.current_range() else {
                return Ok(0);
            };
            let remaining = range.length() - self.range_offset;
            if remaining == 0 {
                if let Some(next) = self.ranges.ranges().get(self.range_index + 1) {
                    self.reader.seek(SeekFrom::Start(next.start()))?;
                }
                self.range_index += 1;
                self.range_offset = 0;
                continue;
            }
            let requested = output
                .len()
                .min(usize::try_from(remaining).unwrap_or(usize::MAX));
            let count = self.reader.read(&mut output[..requested])?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "range ended before its declared length",
                ));
            }
            self.range_offset += count as u64;
            if let Err(error) = self.check_active() {
                output[..count].fill(0);
                return Err(error);
            }
            return Ok(count);
        }
    }
}

impl Seek for VerifiedView<'_> {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.check_active()?;
        let position = match target {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(delta) => self.virtual_position().checked_add_signed(delta),
            SeekFrom::End(delta) => self.info.size.checked_add_signed(delta),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "range seek overflow"))?;
        let position = self.seek_virtual(position)?;
        self.check_active()?;
        Ok(position)
    }
}

impl EvidenceReader for VerifiedView<'_> {
    fn validate_read_scope(&self) -> io::Result<()> {
        self.check_active()
    }
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
    fn read_metrics(&self) -> EvidenceReadMetrics {
        self.reader.read_metrics()
    }
    fn configure_content_scan(&mut self) {
        self.reader.configure_content_scan();
    }
    fn reserve_work(&self, bytes: u64) -> io::Result<Option<Arc<dyn ReaderResourceHold>>> {
        self.check_active()?;
        self.reader.reserve_work(bytes)
    }
    fn preferred_read_granularity(&self) -> usize {
        self.reader.preferred_read_granularity()
    }
}

pub fn check_active(cancel: &AtomicBool, deadline: Option<u64>) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(io::Error::other(RangeReadError::Cancelled));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis();
    if deadline.is_some_and(|deadline| now >= u128::from(deadline)) {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "range read deadline expired",
        ));
    }
    Ok(())
}
