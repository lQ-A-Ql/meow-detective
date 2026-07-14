use super::{CephRocksdbWalFileRecord, CephRocksdbWalRecord};

pub(super) fn map_file(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephRocksdbWalFileRecord> {
    Ok(CephRocksdbWalFileRecord {
        inventory_id: row.get(0)?,
        wal_number: row.get(1)?,
        bluefs_path: row.get(2)?,
        post_manifest: row.get(3)?,
        file_size: row.get(4)?,
        logical_record_count: row.get(5)?,
        empty_batch_count: row.get(6)?,
        mutation_count: row.get(7)?,
        auxiliary_record_count: row.get(8)?,
        logical_payload_bytes: row.get(9)?,
        fragment_count: row.get(10)?,
        first_sequence: row.get(11)?,
        last_sequence: row.get(12)?,
        first_record_offset: row.get(13)?,
        last_record_offset: row.get(14)?,
    })
}

pub(super) fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephRocksdbWalRecord> {
    Ok(CephRocksdbWalRecord {
        inventory_id: row.get(0)?,
        wal_number: row.get(1)?,
        record_ordinal: row.get(2)?,
        physical_offset: row.get(3)?,
        fragment_count: row.get(4)?,
        recyclable_log_number: row.get(5)?,
        batch_sequence: row.get(6)?,
        mutation_count: row.get(7)?,
        auxiliary_record_count: row.get(8)?,
        first_mutation_sequence: row.get(9)?,
        last_mutation_sequence: row.get(10)?,
    })
}
