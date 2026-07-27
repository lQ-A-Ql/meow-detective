use std::io::{self, Read, Seek, SeekFrom};

use super::{EvidenceReader, ReaderInfo};

/// A bounded, zero-based view over one partition in an evidence reader.
pub struct PartitionWindowReader {
    inner: Box<dyn EvidenceReader>,
    info: ReaderInfo,
    start: u64,
    length: u64,
    position: u64,
    preferred_read_granularity: usize,
}

impl PartitionWindowReader {
    /// Builds a window and rejects any declared span outside the evidence source.
    pub fn new(
        inner: Box<dyn EvidenceReader>,
        start: u64,
        length: Option<u64>,
    ) -> io::Result<Self> {
        let source_info = inner.info();
        let available = source_info.size.checked_sub(start).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "partition offset exceeds evidence length",
            )
        })?;
        let length = length.unwrap_or(available);
        if length > available {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "partition length exceeds evidence bounds",
            ));
        }
        let info = ReaderInfo {
            path: source_info.path.clone(),
            size: length,
            kind: format!("partition-window/{}", source_info.kind),
        };
        let preferred_read_granularity = inner.preferred_read_granularity();
        Ok(Self {
            inner,
            info,
            start,
            length,
            position: 0,
            preferred_read_granularity,
        })
    }

    /// Absolute byte offset of the partition in its source.
    #[must_use]
    pub fn source_offset(&self) -> u64 {
        self.start
    }
}

impl Read for PartitionWindowReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.position >= self.length {
            return Ok(0);
        }
        let remaining = self.length - self.position;
        let requested = buf.len().min(remaining as usize);
        let absolute = self.start.checked_add(self.position).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "partition read offset overflow",
            )
        })?;
        self.inner.seek(SeekFrom::Start(absolute))?;
        let read = self.inner.read(&mut buf[..requested])?;
        self.position = self.position.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for PartitionWindowReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.length) + i128::from(delta),
        };
        if !(0..=i128::from(u64::MAX)).contains(&next) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek outside partition address space",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl EvidenceReader for PartitionWindowReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        self.preferred_read_granularity
    }
}
