use crate::RocksDbWireError;

use super::{EntryTypeCounts, SstEntryKind, TableProperties};

#[derive(Default)]
pub(super) struct StreamTotals {
    pub(super) scanned_decompressed_bytes: u64,
    pub(super) counts: EntryTypeCounts,
    pub(super) raw_key_size: u64,
    pub(super) raw_value_size: u64,
    pub(super) smallest_sequence: u64,
    pub(super) largest_sequence: u64,
}

impl StreamTotals {
    pub(super) fn observe_decompressed_bytes(
        &mut self,
        size: usize,
        limit: u64,
    ) -> std::result::Result<(), RocksDbWireError> {
        self.scanned_decompressed_bytes = checked_add_size(
            self.scanned_decompressed_bytes,
            size,
            "SST stream decompressed bytes",
        )?;
        if self.scanned_decompressed_bytes > limit {
            return Err(RocksDbWireError::SstStreamDecompressedLimit { limit });
        }
        Ok(())
    }

    pub(super) fn observe_data(
        &mut self,
        key_size: usize,
        value_size: usize,
        sequence: u64,
        kind: SstEntryKind,
        max_total_entries: u64,
    ) -> std::result::Result<(), RocksDbWireError> {
        self.observe_common(key_size, value_size, sequence, max_total_entries)?;
        match kind {
            SstEntryKind::Deletion
            | SstEntryKind::SingleDeletion
            | SstEntryKind::DeletionWithTimestamp => {
                self.counts.deletions =
                    checked_increment(self.counts.deletions, "SST stream deletion count")?;
            }
            SstEntryKind::Merge => {
                self.counts.merges =
                    checked_increment(self.counts.merges, "SST stream merge count")?;
            }
            SstEntryKind::Value | SstEntryKind::BlobIndex | SstEntryKind::WideColumnEntity => {
                self.counts.values =
                    checked_increment(self.counts.values, "SST stream value count")?;
            }
        }
        Ok(())
    }

    pub(super) fn observe_range(
        &mut self,
        key_size: usize,
        value_size: usize,
        sequence: u64,
        max_total_entries: u64,
        max_range_deletions: u64,
    ) -> std::result::Result<(), RocksDbWireError> {
        self.observe_common(key_size, value_size, sequence, max_total_entries)?;
        self.counts.deletions =
            checked_increment(self.counts.deletions, "SST stream deletion count")?;
        self.counts.range_deletions = checked_increment(
            self.counts.range_deletions,
            "SST stream range deletion count",
        )?;
        if self.counts.range_deletions > max_range_deletions {
            return Err(RocksDbWireError::SstStreamRangeDeletionLimit {
                limit: max_range_deletions,
            });
        }
        Ok(())
    }

    pub(super) fn finish(&mut self) {
        if self.counts.entries == 0 {
            self.smallest_sequence = 0;
        }
    }

    pub(super) fn validate(
        &self,
        properties: &TableProperties,
    ) -> std::result::Result<(), RocksDbWireError> {
        compare_count("entries", self.counts.entries, properties.num_entries)?;
        compare_count("deletions", self.counts.deletions, properties.deleted_keys)?;
        compare_count(
            "merge operands",
            self.counts.merges,
            properties.merge_operands,
        )?;
        compare_count(
            "range deletions",
            self.counts.range_deletions,
            properties.num_range_deletions,
        )?;
        compare_count("raw key bytes", self.raw_key_size, properties.raw_key_size)?;
        compare_count(
            "raw value bytes",
            self.raw_value_size,
            properties.raw_value_size,
        )
    }

    fn observe_common(
        &mut self,
        key_size: usize,
        value_size: usize,
        sequence: u64,
        max_total_entries: u64,
    ) -> std::result::Result<(), RocksDbWireError> {
        let first = self.counts.entries == 0;
        self.counts.entries = checked_increment(self.counts.entries, "SST stream entry count")?;
        if self.counts.entries > max_total_entries {
            return Err(RocksDbWireError::SstStreamEntryLimit {
                limit: max_total_entries,
            });
        }
        self.raw_key_size =
            checked_add_size(self.raw_key_size, key_size, "SST stream raw key bytes")?;
        self.raw_value_size = checked_add_size(
            self.raw_value_size,
            value_size,
            "SST stream raw value bytes",
        )?;
        if first {
            self.smallest_sequence = sequence;
        } else {
            self.smallest_sequence = self.smallest_sequence.min(sequence);
        }
        self.largest_sequence = self.largest_sequence.max(sequence);
        Ok(())
    }
}

pub(super) fn checked_increment(
    value: u64,
    context: &'static str,
) -> std::result::Result<u64, RocksDbWireError> {
    value
        .checked_add(1)
        .ok_or(RocksDbWireError::LengthOverflow { context })
}

fn checked_add_size(
    value: u64,
    size: usize,
    context: &'static str,
) -> std::result::Result<u64, RocksDbWireError> {
    value
        .checked_add(size as u64)
        .ok_or(RocksDbWireError::LengthOverflow { context })
}

fn compare_count(
    field: &'static str,
    parsed: u64,
    properties: u64,
) -> std::result::Result<(), RocksDbWireError> {
    if parsed != properties {
        return Err(RocksDbWireError::SstCountMismatch {
            field,
            parsed,
            properties,
        });
    }
    Ok(())
}
