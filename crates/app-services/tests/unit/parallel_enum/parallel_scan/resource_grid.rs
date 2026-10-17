use super::super::*;
pub(super) use crate::parallel_enum::tests::ntfs_fixture as fixture;
use crate::parallel_enum::{
    batch_sink::prepare_mft_insert,
    ntfs::{
        mft_scan::prepare_mft_scan_from_reader, path_reconstruction::MftCatalog,
        writer::stage_batches,
    },
};
use sha2::{Digest, Sha256};

fn run(bytes: &[u8], workers: usize, window: u64, batch: usize) -> (String, u64, u128, u64, u64) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, bytes).unwrap();
    let factory = crate::source_runtime::open_factory(&path, &domain::DataSourceKind::Raw).unwrap();
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!(
        "../../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::begin(&c, "source", 1)
        .unwrap();
    c.execute_batch("BEGIN").unwrap();
    let profiler = crate::import_pipeline::ImportProfiler::default();
    let mut profile = PhaseRecorder::new(Some(profiler.clone()), Some(1));
    let mut scan =
        prepare_mft_scan_from_reader(factory.open_reader().unwrap(), 0, &mut profile).unwrap();
    let cancel = AtomicBool::new(false);
    let mut catalog = MftCatalog::default();
    let mut stmt = prepare_mft_insert(&c).unwrap();
    let mut next = 0;
    let start = std::time::Instant::now();
    let result = collect_batches_with_budget(
        &mut scan,
        Some(factory.as_ref()),
        workers,
        &cancel,
        &mut profile,
        window,
        |chunk, _| {
            assert_eq!(chunk.start_record, next);
            next += chunk.scanned_records;
            stage_batches(&mut catalog, &mut stmt, &chunk.records, batch, &cancel)?;
            Ok(())
        },
    );
    drop(stmt);
    scan.release_buffer();
    result.unwrap();
    catalog
        .update_staging_paths(&c, "source", 1, &cancel)
        .unwrap();
    c.execute_batch("COMMIT").unwrap();
    let mut query = c.prepare("SELECT * FROM file_entries ORDER BY id").unwrap();
    let columns = query.column_count();
    let mut rows = query.query([]).unwrap();
    let mut hash = Sha256::new();
    while let Some(row) = rows.next().unwrap() {
        for column in 0..columns {
            let value: rusqlite::types::Value = row.get(column).unwrap();
            hash.update(format!("{value:?}\n"));
        }
    }
    drop(profile);
    let report = profiler.report();
    let observed = report
        .samples
        .iter()
        .find(|s| s.phase == Phase::MftReadParseWall)
        .unwrap();
    (
        hex::encode(hash.finalize()),
        next,
        start.elapsed().as_nanos(),
        observed.mft_readers_actual.unwrap(),
        observed.max_reserved_mft_bytes.unwrap(),
    )
}

#[test]
fn workers_bytes_and_writer_batches_preserve_every_catalog_value_and_tail_record() {
    let bytes = fixture::image(20_003);
    let mut expected = None;
    for workers in [1, 2, 4] {
        for window in [64, 128, 256] {
            for batch in [1024, 10_000] {
                let (digest, scanned, _, actual, reserved) =
                    run(&bytes, workers, window * crate::resource_budget::MIB, batch);
                assert_eq!(scanned, 20_003);
                assert!(actual <= workers as u64);
                assert!(reserved <= window * crate::resource_budget::MIB);
                if let Some(expected) = &expected {
                    assert_eq!(&digest, expected)
                } else {
                    expected = Some(digest)
                }
            }
        }
    }
}
#[test]
#[ignore = "explicit native S3 grid; set FORENSICS_MFT_GRID_RECORDS"]
fn native_resource_grid() {
    let records = std::env::var("FORENSICS_MFT_GRID_RECORDS")
        .unwrap()
        .parse()
        .unwrap();
    let bytes = fixture::image(records);
    let mut expected = None;
    let mut conditions = Vec::new();
    for workers in [1, 2, 4] {
        for window in [64, 128, 256] {
            for batch in [1024, 10_000] {
                conditions.push((workers, window, batch));
            }
        }
    }
    for round in 0..5 {
        if round % 2 == 1 {
            conditions.reverse()
        }
        for &(workers, window, batch) in &conditions {
            let (digest, scanned, nanos, actual, reserved) =
                run(&bytes, workers, window * crate::resource_budget::MIB, batch);
            assert_eq!(scanned, records as u64);
            if let Some(expected) = &expected {
                assert_eq!(&digest, expected)
            } else {
                expected = Some(digest.clone())
            }
            eprintln!(
                "[s3-native-grid] {}",
                serde_json::json!({"round":round,"workers":workers,"windowMiB":window,"writerBatch":batch,"actualReaders":actual,"reservedBytes":reserved,"elapsedNs":nanos,"catalogSha256":digest,"scanned":scanned})
            );
        }
    }
}
