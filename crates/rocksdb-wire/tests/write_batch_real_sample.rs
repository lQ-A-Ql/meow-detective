use std::path::{Path, PathBuf};

use rocksdb_wire::{
    decode_log, decode_write_batch, LogDecodeOptions, WriteBatchAuxiliaryKind, WriteBatchLimits,
};

const PVE_WAL_ENV: &str = "FORENSICS_PVE_ROCKSDB_WAL_FIXTURE";

#[test]
#[ignore = "requires one exported private PVE OSD RocksDB WAL fixture"]
fn pve_wal_matches_independent_ldb_oracle() {
    let path = fixture_path();
    assert!(
        path.is_file(),
        "{PVE_WAL_ENV} must name an exported PVE db.wal/*.log fixture"
    );
    let oracle = oracle_for(&path);
    let bytes = std::fs::read(&path).expect("read exported RocksDB WAL");
    assert_eq!(bytes.len(), oracle.file_size);

    let records = decode_log(
        &bytes,
        LogDecodeOptions {
            expected_recyclable_log_number: Some(oracle.file_number),
            ..LogDecodeOptions::default()
        },
    )
    .expect("decode physical RocksDB WAL");
    assert_eq!(records.len(), oracle.logical_record_count);
    assert_eq!(
        records
            .iter()
            .map(|record| record.data.len())
            .sum::<usize>(),
        oracle.logical_payload_bytes
    );
    assert_eq!(records.first().expect("first record").physical_offset, 0);
    assert_eq!(
        records.last().expect("last record").physical_offset,
        oracle.last_record_offset
    );

    let batches = records
        .iter()
        .map(|record| {
            decode_write_batch(&record.data, WriteBatchLimits::default())
                .expect("decode RocksDB WriteBatch")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        batches
            .iter()
            .filter(|batch| batch.mutations.is_empty())
            .count(),
        oracle.empty_batch_count
    );
    assert_eq!(
        batches
            .iter()
            .map(|batch| u64::from(batch.declared_count))
            .sum::<u64>(),
        oracle.mutation_count
    );
    assert_eq!(
        batches.first().expect("first batch").sequence,
        oracle.first_sequence
    );
    assert_eq!(
        batches
            .iter()
            .find_map(|batch| batch.mutations.first())
            .expect("first mutation")
            .sequence,
        oracle.first_sequence
    );
    assert_eq!(
        batches.iter().rev().find_map(|batch| batch.last_sequence()),
        Some(oracle.last_sequence)
    );
    assert!(
        batches.iter().all(|batch| batch
            .auxiliary_records
            .iter()
            .all(|record| record.kind != WriteBatchAuxiliaryKind::Noop)),
        "Ceph recovery profile does not support RocksDB seq_per_batch Noop records"
    );
}

fn fixture_path() -> PathBuf {
    std::env::var_os(PVE_WAL_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("__missing_pve_rocksdb_wal_fixture__").to_path_buf())
}

fn oracle_for(path: &Path) -> WalOracle {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("000120.log") => WalOracle {
            file_number: 120,
            file_size: 4_142_839,
            logical_record_count: 3_782,
            empty_batch_count: 1_084,
            mutation_count: 9_644,
            logical_payload_bytes: 4_115_489,
            first_sequence: 1_052_659,
            last_sequence: 1_062_302,
            last_record_offset: 4_142_625,
        },
        Some("000127.log") => WalOracle {
            file_number: 127,
            file_size: 4_145_432,
            logical_record_count: 3_812,
            empty_batch_count: 1_112,
            mutation_count: 9_644,
            logical_payload_bytes: 4_117_873,
            first_sequence: 1_061_240,
            last_sequence: 1_070_883,
            last_record_offset: 4_145_218,
        },
        Some("000142.log") => WalOracle {
            file_number: 142,
            file_size: 3_921_274,
            logical_record_count: 3_710,
            empty_batch_count: 1_107,
            mutation_count: 9_338,
            logical_payload_bytes: 3_894_471,
            first_sequence: 1_077_118,
            last_sequence: 1_086_455,
            last_record_offset: 3_921_060,
        },
        _ => panic!("{PVE_WAL_ENV} does not name a recognized PVE WAL fixture"),
    }
}

struct WalOracle {
    file_number: u32,
    file_size: usize,
    logical_record_count: usize,
    empty_batch_count: usize,
    mutation_count: u64,
    logical_payload_bytes: usize,
    first_sequence: u64,
    last_sequence: u64,
    last_record_offset: u64,
}
