use serde::{Deserialize, Deserializer, Serialize};

use super::{validate_partition, CoverageError};
use crate::structure::ByteRange;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteCoverage {
    pub target_size: u64,
    pub requested: Vec<ByteRange>,
    pub verified: Vec<ByteRange>,
    pub native_holes: Vec<ByteRange>,
    pub unknown: Vec<ByteRange>,
    pub unreadable: Vec<ByteRange>,
}

impl ByteCoverage {
    pub fn validate(&self) -> Result<(), CoverageError> {
        if self.requested.is_empty() && self.target_size != 0 {
            return Err(CoverageError::EmptyRequest);
        }
        validate_partition(
            &self.intervals(&self.requested)?,
            &[
                self.intervals(&self.verified)?,
                self.intervals(&self.native_holes)?,
                self.intervals(&self.unknown)?,
                self.intervals(&self.unreadable)?,
            ],
        )
    }

    pub fn verified_bytes(&self) -> Result<u64, CoverageError> {
        self.validate()?;
        Ok(self.verified.iter().map(|range| range.length()).sum())
    }

    pub fn native_hole_bytes(&self) -> Result<u64, CoverageError> {
        self.validate()?;
        Ok(self.native_holes.iter().map(|range| range.length()).sum())
    }

    fn intervals(&self, ranges: &[ByteRange]) -> Result<Vec<(u64, u64)>, CoverageError> {
        ranges
            .iter()
            .map(|range| {
                let end = range
                    .start()
                    .checked_add(range.length())
                    .ok_or(CoverageError::InvalidRange)?;
                if range.is_empty() {
                    return Err(CoverageError::InvalidRange);
                }
                if end > self.target_size {
                    return Err(CoverageError::OutOfBounds);
                }
                Ok((range.start(), end))
            })
            .collect()
    }
}

impl<'de> Deserialize<'de> for ByteCoverage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            target_size: u64,
            requested: Vec<ByteRange>,
            verified: Vec<ByteRange>,
            native_holes: Vec<ByteRange>,
            unknown: Vec<ByteRange>,
            unreadable: Vec<ByteRange>,
        }
        let record = Record::deserialize(deserializer)?;
        let value = Self {
            target_size: record.target_size,
            requested: record.requested,
            verified: record.verified,
            native_holes: record.native_holes,
            unknown: record.unknown,
            unreadable: record.unreadable,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
