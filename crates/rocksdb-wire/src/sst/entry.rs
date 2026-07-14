use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::RocksDbWireError;

use super::{BlockHandle, EntryTypeCounts, SstReadOptions, TableProperties};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SstEntryKind {
    Deletion,
    Value,
    Merge,
    SingleDeletion,
    BlobIndex,
    DeletionWithTimestamp,
    WideColumnEntity,
}

impl SstEntryKind {
    pub fn value_type(self) -> u8 {
        match self {
            Self::Deletion => 0x00,
            Self::Value => 0x01,
            Self::Merge => 0x02,
            Self::SingleDeletion => 0x07,
            Self::BlobIndex => 0x11,
            Self::DeletionWithTimestamp => 0x14,
            Self::WideColumnEntity => 0x16,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SstDataEntry<'a> {
    pub column_family_id: u32,
    pub block_handle: BlockHandle,
    pub block_ordinal: u64,
    pub entry_ordinal: u64,
    pub internal_key: &'a [u8],
    pub user_key: &'a [u8],
    pub sequence: u64,
    pub kind: SstEntryKind,
    pub value: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct SstRangeDeletionEntry<'a> {
    pub column_family_id: u32,
    pub block_handle: BlockHandle,
    pub entry_ordinal: u64,
    pub internal_key: &'a [u8],
    pub start_user_key: &'a [u8],
    pub end_user_key: &'a [u8],
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SstVisitOptions {
    pub read: SstReadOptions,
    pub max_data_blocks: u64,
    pub max_total_entries: u64,
    pub max_range_deletions: u64,
    pub max_total_decompressed_bytes: u64,
}

impl Default for SstVisitOptions {
    fn default() -> Self {
        Self {
            read: SstReadOptions::default(),
            max_data_blocks: 1_000_000,
            max_total_entries: 2_000_000,
            max_range_deletions: 100_000,
            max_total_decompressed_bytes: 1024 * MIB,
        }
    }
}

/// Receives borrowed SST records. Callbacks are provisional until
/// `visit_sst_entries` returns `Ok`; callers must roll back staged output on error.
pub trait SstEntryVisitor {
    type Error;

    fn visit_data(&mut self, entry: SstDataEntry<'_>) -> std::result::Result<(), Self::Error>;

    fn visit_range_deletion(
        &mut self,
        entry: SstRangeDeletionEntry<'_>,
    ) -> std::result::Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum SstVisitError<E> {
    Wire(RocksDbWireError),
    Visitor(E),
}

impl<E: Display> Display for SstVisitError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wire(error) => Display::fmt(error, formatter),
            Self::Visitor(error) => write!(formatter, "SST entry visitor failed: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for SstVisitError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Wire(error) => Some(error),
            Self::Visitor(error) => Some(error),
        }
    }
}

impl<E> From<RocksDbWireError> for SstVisitError<E> {
    fn from(error: RocksDbWireError) -> Self {
        Self::Wire(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SstEntryStreamSummary {
    pub file_size: u64,
    pub properties: TableProperties,
    pub data_block_count: u64,
    pub scanned_decompressed_bytes: u64,
    pub counts: EntryTypeCounts,
    pub raw_key_size: u64,
    pub raw_value_size: u64,
    pub smallest_sequence: u64,
    pub largest_sequence: u64,
}
