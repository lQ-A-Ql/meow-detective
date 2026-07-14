use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::RocksDbWireError;

const MIB: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatestStateLimits {
    pub max_versions: usize,
    pub max_key_history_bytes: usize,
    pub max_merge_operands: usize,
    pub max_resolved_value_bytes: usize,
}

impl Default for LatestStateLimits {
    fn default() -> Self {
        Self {
            max_versions: 100_000,
            max_key_history_bytes: 64 * MIB,
            max_merge_operands: 100_000,
            max_resolved_value_bytes: 64 * MIB,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyVersionKind<'a> {
    Value { value: &'a [u8] },
    Delete,
    SingleDelete,
    Merge { operand: &'a [u8] },
}

impl KeyVersionKind<'_> {
    pub(crate) fn value_type(self) -> u8 {
        match self {
            Self::Delete => 0x00,
            Self::Value { .. } => 0x01,
            Self::Merge { .. } => 0x02,
            Self::SingleDelete => 0x07,
        }
    }

    pub(crate) fn payload_length(self) -> usize {
        match self {
            Self::Value { value } => value.len(),
            Self::Merge { operand } => operand.len(),
            Self::Delete | Self::SingleDelete => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyVersion<'a> {
    pub sequence: u64,
    pub kind: KeyVersionKind<'a>,
}

impl<'a> KeyVersion<'a> {
    pub const fn value(sequence: u64, value: &'a [u8]) -> Self {
        Self {
            sequence,
            kind: KeyVersionKind::Value { value },
        }
    }

    pub const fn delete(sequence: u64) -> Self {
        Self {
            sequence,
            kind: KeyVersionKind::Delete,
        }
    }

    pub const fn single_delete(sequence: u64) -> Self {
        Self {
            sequence,
            kind: KeyVersionKind::SingleDelete,
        }
    }

    pub const fn merge(sequence: u64, operand: &'a [u8]) -> Self {
        Self {
            sequence,
            kind: KeyVersionKind::Merge { operand },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LatestState {
    Value { sequence: u64, value: Vec<u8> },
    Delete { sequence: u64 },
    SingleDelete { sequence: u64 },
    RangeDelete { sequence: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LatestStateRef<'a> {
    Value { sequence: u64, value: Cow<'a, [u8]> },
    Delete { sequence: u64 },
    SingleDelete { sequence: u64 },
    RangeDelete { sequence: u64 },
}

impl LatestStateRef<'_> {
    pub fn into_owned(self) -> LatestState {
        match self {
            Self::Value { sequence, value } => LatestState::Value {
                sequence,
                value: value.into_owned(),
            },
            Self::Delete { sequence } => LatestState::Delete { sequence },
            Self::SingleDelete { sequence } => LatestState::SingleDelete { sequence },
            Self::RangeDelete { sequence } => LatestState::RangeDelete { sequence },
        }
    }
}

/// Resolves RocksDB merge operands without assigning wire-level semantics to them.
pub trait MergeOperator {
    type Error;

    fn full_merge(
        &mut self,
        user_key: &[u8],
        existing_value: Option<&[u8]>,
        operands_oldest_to_newest: &[&[u8]],
        max_output_bytes: usize,
    ) -> std::result::Result<Vec<u8>, Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum LatestStateError<E> {
    Wire(RocksDbWireError),
    MergeOperator(E),
}

impl<E: Display> Display for LatestStateError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wire(error) => Display::fmt(error, formatter),
            Self::MergeOperator(error) => {
                write!(formatter, "RocksDB merge operator failed: {error}")
            }
        }
    }
}

impl<E: Error + 'static> Error for LatestStateError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Wire(error) => Some(error),
            Self::MergeOperator(error) => Some(error),
        }
    }
}

impl<E> From<RocksDbWireError> for LatestStateError<E> {
    fn from(error: RocksDbWireError) -> Self {
        Self::Wire(error)
    }
}
