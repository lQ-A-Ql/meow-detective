use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ByteRange {
    start: u64,
    length: u64,
}

impl<'de> Deserialize<'de> for ByteRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Record {
            start: u64,
            length: u64,
        }
        let record = Record::deserialize(deserializer)?;
        Self::new(record.start, record.length).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ByteRangeError {
    #[error("byte range exceeds the address space")]
    Overflow,
    #[error("byte range exceeds target length {logical_size}")]
    OutOfBounds { logical_size: u64 },
}

impl ByteRange {
    pub fn new(start: u64, length: u64) -> Result<Self, ByteRangeError> {
        start.checked_add(length).ok_or(ByteRangeError::Overflow)?;
        Ok(Self { start, length })
    }

    pub fn validate(self, logical_size: u64) -> Result<(), ByteRangeError> {
        if self.end() > logical_size {
            return Err(ByteRangeError::OutOfBounds { logical_size });
        }
        Ok(())
    }

    pub fn start(self) -> u64 {
        self.start
    }

    pub fn length(self) -> u64 {
        self.length
    }

    pub fn end(self) -> u64 {
        self.start + self.length
    }

    pub fn is_empty(self) -> bool {
        self.length == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRangeSet {
    target_size: u64,
    ranges: Vec<ByteRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ByteRangeSetError {
    #[error(transparent)]
    Range(#[from] ByteRangeError),
    #[error("read demand contains no ranges")]
    Empty,
    #[error("zero-length ranges are only valid for an empty target")]
    EmptyRange,
}

impl ByteRangeSet {
    pub fn new(
        target_size: u64,
        ranges: impl IntoIterator<Item = ByteRange>,
    ) -> Result<Self, ByteRangeSetError> {
        let mut ranges = ranges.into_iter().collect::<Vec<_>>();
        if ranges.is_empty() {
            return Err(ByteRangeSetError::Empty);
        }
        for range in &ranges {
            range.validate(target_size)?;
            if range.is_empty() && target_size != 0 {
                return Err(ByteRangeSetError::EmptyRange);
            }
        }
        ranges.sort_unstable();
        let mut merged: Vec<ByteRange> = Vec::with_capacity(ranges.len());
        for range in ranges {
            let Some(previous) = merged.last_mut() else {
                merged.push(range);
                continue;
            };
            if range.start() <= previous.end() {
                let end = previous.end().max(range.end());
                *previous = ByteRange::new(previous.start(), end - previous.start())
                    .map_err(ByteRangeSetError::Range)?;
            } else {
                merged.push(range);
            }
        }
        Ok(Self {
            target_size,
            ranges: merged,
        })
    }

    pub fn full(target_size: u64) -> Result<Self, ByteRangeSetError> {
        Self::new(target_size, [ByteRange::new(0, target_size)?])
    }

    pub fn target_size(&self) -> u64 {
        self.target_size
    }

    pub fn ranges(&self) -> &[ByteRange] {
        &self.ranges
    }

    pub fn covered_bytes(&self) -> u64 {
        self.ranges.iter().map(|range| range.length()).sum()
    }

    pub fn is_full(&self) -> bool {
        self.ranges.len() == 1
            && self.ranges[0].start() == 0
            && self.ranges[0].length() == self.target_size
    }

    pub fn validate(&self) -> Result<(), ByteRangeSetError> {
        Self::new(self.target_size, self.ranges.clone()).map(|_| ())
    }
}

impl<'de> Deserialize<'de> for ByteRangeSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            target_size: u64,
            ranges: Vec<ByteRange>,
        }
        let record = Record::deserialize(deserializer)?;
        Self::new(record.target_size, record.ranges).map_err(serde::de::Error::custom)
    }
}
