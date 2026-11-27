use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{atomic::AtomicBool, Arc},
};

use domain::structure::ByteRangeSet;
use evidence_core::{
    reader::{EvidenceReadMetrics, ReaderResourceHold},
    EvidenceReader, ReaderInfo,
};

use super::plan::check_cancel;

pub struct VerifiedView<'a> {
    reader: Box<dyn EvidenceReader>,
    ranges: ByteRangeSet,
    range_index: usize,
    range_offset: u64,
    info: ReaderInfo,
    cancel: &'a AtomicBool,
}

impl<'a> VerifiedView<'a> {
    pub(super) fn new(
        mut reader: Box<dyn EvidenceReader>,
        ranges: ByteRangeSet,
        cancel: &'a AtomicBool,
    ) -> Self {
        let mut info = reader.info().clone();
        info.size = ranges.covered_bytes();
        let initial = ranges
            .ranges()
            .first()
            .map_or((0, 0), |range| (0, range.start()));
        if let Err(error) = reader.seek(SeekFrom::Start(initial.1)) {
            info.size = 0;
            drop(error);
        }
        Self {
            reader,
            ranges,
            range_index: initial.0,
            range_offset: 0,
            info,
            cancel,
        }
    }

    fn check_cancel(&self) -> io::Result<()> {
        check_cancel(self.cancel).map_err(io::Error::other)
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
                "verified view seek exceeds covered bytes",
            ));
        }
        if position == self.info.size {
            self.range_index = self.ranges.ranges().len();
            self.range_offset = 0;
            return Ok(position);
        }
        let mut remaining = position;
        for (index, range) in self.ranges.ranges().iter().enumerate() {
            if remaining < range.length() || range.length() == 0 {
                self.range_index = index;
                self.range_offset = remaining;
                self.reader
                    .seek(SeekFrom::Start(range.start() + remaining))?;
                return Ok(position);
            }
            remaining -= range.length();
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "verified view seek is outside its ranges",
        ))
    }
}

impl Read for VerifiedView<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.check_cancel()?;
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let Some(range) = self.current_range() else {
                return Ok(0);
            };
            let remaining = range.length().saturating_sub(self.range_offset);
            if remaining == 0 {
                self.range_index += 1;
                self.range_offset = 0;
                if let Some(next) = self.current_range() {
                    self.reader.seek(SeekFrom::Start(next.start()))?;
                }
                continue;
            }
            let requested = output
                .len()
                .min(usize::try_from(remaining).unwrap_or(usize::MAX));
            let count = self.reader.read(&mut output[..requested])?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "verified range ended before its declared length",
                ));
            }
            self.range_offset += count as u64;
            self.check_cancel()?;
            return Ok(count);
        }
    }
}

impl Seek for VerifiedView<'_> {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.check_cancel()?;
        let position = match target {
            SeekFrom::Start(position) => position,
            SeekFrom::Current(delta) => {
                let current = self.virtual_position();
                current
                    .checked_add_signed(delta)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek overflow"))?
            }
            SeekFrom::End(delta) => self
                .info
                .size
                .checked_add_signed(delta)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek overflow"))?,
        };
        self.seek_virtual(position)
    }
}

impl EvidenceReader for VerifiedView<'_> {
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
        self.check_cancel()?;
        self.reader.reserve_work(bytes)
    }

    fn preferred_read_granularity(&self) -> usize {
        self.reader.preferred_read_granularity()
    }
}
