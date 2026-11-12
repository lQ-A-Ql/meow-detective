use super::*;
use crate::parallel_enum::ntfs::writer::stage_batches;
use persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo as Repo;

fn records(count: usize) -> Vec<MftRecord> {
    std::iter::once(fake_mft_record(5, 5, ".", true))
        .chain(
            (16..count).map(|id| fake_mft_record(id as u64, 5, &format!("file-{id}.txt"), false)),
        )
        .collect()
}
#[test]
fn writer_batch_tail_empty_invalid_and_cancel_keep_counts_and_transaction_ownership() {
    let input = records(10_021);
    let mut reference = None;
    for batch in [1024, 2048, 4096, 10000] {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!(
            "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
        ))
        .unwrap();
        Repo::begin(&c, "source", 1).unwrap();
        c.execute_batch("BEGIN").unwrap();
        let mut statement = prepare_mft_insert(&c).unwrap();
        let mut catalog = MftCatalog::default();
        let written = stage_batches(
            &mut catalog,
            &mut statement,
            &input,
            batch,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(written.rows, input.len() as u64);
        assert_eq!(written.batches, input.len().div_ceil(batch) as u64);
        assert!(written.maximum_records <= batch as u64);
        assert!(!c.is_autocommit());
        let empty = stage_batches(
            &mut catalog,
            &mut statement,
            &[],
            batch,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            (empty.rows, empty.batches, empty.maximum_records),
            (0, 0, 0)
        );
        assert!(stage_batches(
            &mut catalog,
            &mut statement,
            &input,
            0,
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(matches!(
            stage_batches(
                &mut catalog,
                &mut statement,
                &input,
                batch,
                &AtomicBool::new(true)
            ),
            Err(ParallelEnumError::Cancelled)
        ));
        let stats = catalog.stats();
        if let Some(old) = reference {
            assert_eq!((stats.file_count, stats.dir_count, stats.total_size), old);
        } else {
            reference = Some((stats.file_count, stats.dir_count, stats.total_size));
        }
        drop(statement);
        c.execute_batch("ROLLBACK").unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM ntfs_entries", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
#[ignore = "explicit Release writer experiment; set FORENSICS_MFT_WRITER_RECORDS"]
fn native_writer_batch_cost() {
    let count: usize = std::env::var("FORENSICS_MFT_WRITER_RECORDS")
        .expect("set records")
        .parse()
        .unwrap();
    assert!((32..=1_000_000).contains(&count));
    let input = records(count);
    let mut result = Vec::new();
    for round in 0..5 {
        let sizes = if round % 2 == 0 {
            [1024, 2048, 4096, 10000]
        } else {
            [10000, 4096, 2048, 1024]
        };
        for batch in sizes {
            let temp = tempfile::tempdir().unwrap();
            let c = staging::open_partition_staging(temp.path(), "writer", 1).unwrap();
            let tx = c.unchecked_transaction().unwrap();
            Repo::begin(&tx, "writer", 1).unwrap();
            let mut statement = prepare_mft_insert(&tx).unwrap();
            let mut catalog = MftCatalog::default();
            let start = Instant::now();
            let stats = stage_batches(
                &mut catalog,
                &mut statement,
                &input,
                batch,
                &AtomicBool::new(false),
            )
            .unwrap();
            let stage_ns = start.elapsed().as_nanos();
            assert_eq!(stats.rows, input.len() as u64);
            drop(statement);
            let commit = Instant::now();
            tx.commit().unwrap();
            let commit_ns = commit.elapsed().as_nanos();
            let actual: u64 = c
                .query_row("SELECT COUNT(*) FROM ntfs_entries", [], |r| r.get(0))
                .unwrap();
            assert_eq!(actual, input.len() as u64);
            result.push(serde_json::json!({"round":round,"batchRecords":batch,"rows":actual,"stageNs":stage_ns,"commitNs":commit_ns,"writerBatches":stats.batches,"maximumBatchRecords":stats.maximum_records}));
        }
    }
    eprintln!(
        "[native-writer-cost] {}",
        serde_json::json!({"version":1,"records":count,"scope":"5 alternating rounds; native MftCatalog bookkeeping and 17-column cached writer; same partition transaction; not full workflow","results":result})
    );
}
