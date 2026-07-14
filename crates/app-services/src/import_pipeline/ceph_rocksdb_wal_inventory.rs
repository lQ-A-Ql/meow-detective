use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use persistence_sqlite::repositories::ceph_rocksdb_repo::CephRocksdbAggregate;
use persistence_sqlite::repositories::ceph_rocksdb_wal_repo::{
    CephRocksdbWalAggregate, CephRocksdbWalFileRecord, CephRocksdbWalRecord,
};
use rocksdb_wire::{
    decode_log, decode_write_batch, LogDecodeOptions, RocksDbWireError, WriteBatchAuxiliaryKind,
    WriteBatchLimits,
};
use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_rocksdb_wal_locator::{LocatedRocksdbWal, RocksdbWalSelection};

pub(super) fn inventory_active_rocksdb_wals(
    reader: &mut BluefsExtentReader<'_>,
    selection: &RocksdbWalSelection<'_>,
    rocksdb: &CephRocksdbAggregate,
    cancel_token: &AtomicBool,
) -> Result<CephRocksdbWalAggregate, CommandError> {
    let column_families = ColumnFamilyInventory::from_rocksdb(rocksdb);
    let mut files = Vec::with_capacity(selection.files.len());
    let mut records = Vec::new();
    let mut sequence_state = WalSequenceState::default();
    for located in &selection.files {
        ensure_not_cancelled(cancel_token)?;
        if located.wal_number < selection.recovery_lower_bound {
            return Err(inventory_error(format!(
                "selected WAL {} is below recovery lower bound {}",
                located.wal_number, selection.recovery_lower_bound
            )));
        }
        let bytes = if located.file.fnode.size == 0 {
            Vec::new()
        } else {
            reader.read_plain_file(&located.file.fnode)?
        };
        // RocksDB stores only the low 32 bits in recyclable physical headers.
        let recyclable_identity = u32::try_from(located.wal_number & u64::from(u32::MAX))
            .map_err(|_| inventory_error("RocksDB recyclable WAL identity conversion failed"))?;
        let decoded = decode_log(
            &bytes,
            LogDecodeOptions {
                expected_recyclable_log_number: Some(recyclable_identity),
                ..LogDecodeOptions::default()
            },
        )
        .map_err(map_wal_error)?;
        let start = records.len();
        let file = inventory_wal_records(
            &decoded,
            rocksdb,
            located,
            &column_families,
            cancel_token,
            &mut sequence_state,
            &mut records,
        )?;
        if records.len() - start != file.logical_record_count as usize {
            return Err(inventory_error(
                "RocksDB WAL record inventory count is inconsistent",
            ));
        }
        files.push(file);
    }
    Ok(CephRocksdbWalAggregate { files, records })
}

fn inventory_wal_records(
    decoded: &[rocksdb_wire::LogicalLogRecord],
    rocksdb: &CephRocksdbAggregate,
    located: &LocatedRocksdbWal<'_>,
    column_families: &ColumnFamilyInventory,
    cancel_token: &AtomicBool,
    sequence_state: &mut WalSequenceState,
    output: &mut Vec<CephRocksdbWalRecord>,
) -> Result<CephRocksdbWalFileRecord, CommandError> {
    let mut summary = WalSummary::default();
    for (index, record) in decoded.iter().enumerate() {
        if index % 1024 == 0 {
            ensure_not_cancelled(cancel_token)?;
        }
        let batch =
            decode_write_batch(&record.data, WriteBatchLimits::default()).map_err(map_wal_error)?;
        validate_record_ordinal(record, index)?;
        validate_auxiliary_records(&batch)?;
        validate_batch_sequence(&batch, sequence_state)?;
        validate_column_families(&batch, column_families)?;
        summary.observe(record, &batch)?;
        output.push(CephRocksdbWalRecord {
            inventory_id: rocksdb.manifest.inventory_id.clone(),
            wal_number: located.wal_number,
            record_ordinal: record.ordinal,
            physical_offset: record.physical_offset,
            fragment_count: record.fragment_count,
            recyclable_log_number: record.recyclable_log_number,
            batch_sequence: batch.sequence,
            mutation_count: batch.declared_count,
            auxiliary_record_count: batch.auxiliary_record_count,
            first_mutation_sequence: batch.mutations.first().map(|mutation| mutation.sequence),
            last_mutation_sequence: batch.last_sequence(),
        });
    }
    summary.finish(rocksdb, located)
}

fn validate_record_ordinal(
    record: &rocksdb_wire::LogicalLogRecord,
    index: usize,
) -> Result<(), CommandError> {
    let expected =
        u64::try_from(index).map_err(|_| inventory_error("WAL record ordinal exceeds u64"))?;
    if record.ordinal != expected {
        return Err(inventory_error(format!(
            "WAL record ordinal {} does not match expected {expected}",
            record.ordinal
        )));
    }
    Ok(())
}

fn validate_batch_sequence(
    batch: &rocksdb_wire::WriteBatch<'_>,
    state: &mut WalSequenceState,
) -> Result<(), CommandError> {
    if state
        .previous_batch_sequence
        .is_some_and(|previous| batch.sequence < previous)
    {
        return Err(inventory_error(format!(
            "WAL batch sequence {} regresses below previous batch sequence {}",
            batch.sequence,
            state.previous_batch_sequence.unwrap_or_default()
        )));
    }
    state.previous_batch_sequence = Some(batch.sequence);
    if let Some(last) = batch.last_sequence() {
        if state
            .next_non_overlapping_mutation_sequence
            .is_some_and(|next| batch.sequence < next)
        {
            return Err(inventory_error(format!(
                "WAL mutation sequence {} overlaps an earlier batch ending at {}",
                batch.sequence,
                state
                    .next_non_overlapping_mutation_sequence
                    .unwrap_or_default()
                    .saturating_sub(1)
            )));
        }
        state.next_non_overlapping_mutation_sequence = Some(
            last.checked_add(1)
                .ok_or_else(|| inventory_error("WAL sequence boundary overflow"))?,
        );
    }
    Ok(())
}

fn validate_auxiliary_records(batch: &rocksdb_wire::WriteBatch<'_>) -> Result<(), CommandError> {
    if batch
        .auxiliary_records
        .iter()
        .any(|record| record.kind == WriteBatchAuxiliaryKind::Noop)
    {
        return Err(CommandError::unsupported(
            "RocksDB WAL Noop records require seq_per_batch recovery semantics",
        ));
    }
    Ok(())
}

fn validate_column_families(
    batch: &rocksdb_wire::WriteBatch<'_>,
    column_families: &ColumnFamilyInventory,
) -> Result<(), CommandError> {
    if let Some(mutation) = batch.mutations.iter().find(|mutation| {
        !column_families.active.contains(&mutation.column_family_id)
            && !column_families.dropped.contains(&mutation.column_family_id)
    }) {
        return Err(inventory_error(format!(
            "WAL mutation references unknown column family {}",
            mutation.column_family_id
        )));
    }
    Ok(())
}

#[derive(Default)]
struct WalSequenceState {
    previous_batch_sequence: Option<u64>,
    next_non_overlapping_mutation_sequence: Option<u64>,
}

struct ColumnFamilyInventory {
    active: HashSet<u32>,
    dropped: HashSet<u32>,
}

impl ColumnFamilyInventory {
    fn from_rocksdb(rocksdb: &CephRocksdbAggregate) -> Self {
        let mut active = HashSet::new();
        let mut dropped = HashSet::new();
        for column_family in &rocksdb.column_families {
            if column_family.dropped {
                dropped.insert(column_family.column_family_id);
            } else {
                active.insert(column_family.column_family_id);
            }
        }
        Self { active, dropped }
    }
}

#[derive(Default)]
struct WalSummary {
    logical_record_count: u32,
    empty_batch_count: u32,
    mutation_count: u64,
    auxiliary_record_count: u64,
    logical_payload_bytes: u64,
    fragment_count: u64,
    first_sequence: Option<u64>,
    last_sequence: Option<u64>,
    first_record_offset: Option<u64>,
    last_record_offset: Option<u64>,
}

impl WalSummary {
    fn observe(
        &mut self,
        record: &rocksdb_wire::LogicalLogRecord,
        batch: &rocksdb_wire::WriteBatch<'_>,
    ) -> Result<(), CommandError> {
        self.logical_record_count = self
            .logical_record_count
            .checked_add(1)
            .ok_or_else(|| inventory_error("WAL logical record count overflow"))?;
        if batch.mutations.is_empty() {
            self.empty_batch_count = self
                .empty_batch_count
                .checked_add(1)
                .ok_or_else(|| inventory_error("WAL empty batch count overflow"))?;
        }
        self.mutation_count = self
            .mutation_count
            .checked_add(u64::from(batch.declared_count))
            .ok_or_else(|| inventory_error("WAL mutation count overflow"))?;
        self.auxiliary_record_count = self
            .auxiliary_record_count
            .checked_add(u64::from(batch.auxiliary_record_count))
            .ok_or_else(|| inventory_error("WAL auxiliary record count overflow"))?;
        self.logical_payload_bytes = self
            .logical_payload_bytes
            .checked_add(record.data.len() as u64)
            .ok_or_else(|| inventory_error("WAL logical payload byte count overflow"))?;
        self.fragment_count = self
            .fragment_count
            .checked_add(u64::from(record.fragment_count))
            .ok_or_else(|| inventory_error("WAL fragment count overflow"))?;
        self.first_sequence.get_or_insert(batch.sequence);
        self.last_sequence = Some(batch.last_sequence().unwrap_or(batch.sequence));
        self.first_record_offset
            .get_or_insert(record.physical_offset);
        self.last_record_offset = Some(record.physical_offset);
        Ok(())
    }

    fn finish(
        self,
        rocksdb: &CephRocksdbAggregate,
        located: &LocatedRocksdbWal<'_>,
    ) -> Result<CephRocksdbWalFileRecord, CommandError> {
        if self.empty_batch_count > self.logical_record_count {
            return Err(inventory_error(
                "WAL empty batch count exceeds logical record count",
            ));
        }
        Ok(CephRocksdbWalFileRecord {
            inventory_id: rocksdb.manifest.inventory_id.clone(),
            wal_number: located.wal_number,
            bluefs_path: located.path.clone(),
            post_manifest: located.post_manifest,
            file_size: located.file.fnode.size,
            logical_record_count: self.logical_record_count,
            empty_batch_count: self.empty_batch_count,
            mutation_count: self.mutation_count,
            auxiliary_record_count: self.auxiliary_record_count,
            logical_payload_bytes: self.logical_payload_bytes,
            fragment_count: self.fragment_count,
            first_sequence: self.first_sequence,
            last_sequence: self.last_sequence,
            first_record_offset: self.first_record_offset,
            last_record_offset: self.last_record_offset,
        })
    }
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), CommandError> {
    if cancel_token.load(Ordering::Relaxed) {
        return Err(CommandError::conflict(
            "Import cancelled during RocksDB WAL inventory",
        ));
    }
    Ok(())
}

fn map_wal_error(error: RocksDbWireError) -> CommandError {
    let message = format!("RocksDB WAL decode failed: {error}");
    match error {
        RocksDbWireError::UnsupportedWalCompressionRecord { .. }
        | RocksDbWireError::UnsupportedWriteBatchTag { .. } => CommandError::unsupported(message),
        _ => CommandError::parser(message),
    }
}

fn inventory_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("RocksDB WAL inventory failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_wal_inventory.rs"]
mod tests;
