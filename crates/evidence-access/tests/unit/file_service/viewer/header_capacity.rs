use super::*;
use std::time::{Duration, Instant};

const CASE_ID: &str = "case-header-real-capacity";
const SOURCE_ID: &str = "source-header-real-capacity";

struct WorkerResult {
    files_read: usize,
    peak_entries: usize,
    final_entries: usize,
    sampled_peak_rss_mb: u64,
}

fn prepare_files(count: usize) -> (tempfile::TempDir, NativeSourceFixture) {
    let started = Instant::now();
    let root = tempfile::tempdir().unwrap();
    let evidence = root.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    let fixture = native_logical_source(
        SOURCE_ID,
        CASE_ID,
        evidence.clone(),
        DataSourceKind::LogicalDirectory,
    );
    log_phase("initialize", 0, count, started.elapsed(), started.elapsed());
    let mut file_creation = Duration::ZERO;
    let mut catalog_write = Duration::ZERO;
    for start in (0..count).step_by(1024) {
        let batch_started = Instant::now();
        let mut records = Vec::new();
        for index in start..(start + 1024).min(count) {
            let name = format!("{index:06}.bin");
            std::fs::write(evidence.join(&name), (index as u64).to_le_bytes()).unwrap();
            records.push(FileEntry {
                id: FileEntryId(format!("header-{index:06}")),
                parent_id: None,
                data_source_id: DataSourceId(SOURCE_ID.into()),
                path: name.clone(),
                name,
                entry_type: EntryType::File,
                size: Some(8),
                ext: Some("bin".into()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            });
        }
        let completed = (start + 1024).min(count);
        let elapsed = batch_started.elapsed();
        file_creation += elapsed;
        log_phase("file-create", completed, count, elapsed, started.elapsed());
        let batch_started = Instant::now();
        FileRepo::new(&fixture.connection)
            .insert_batch(&records)
            .unwrap();
        let elapsed = batch_started.elapsed();
        catalog_write += elapsed;
        log_phase(
            "catalog-write",
            completed,
            count,
            elapsed,
            started.elapsed(),
        );
    }
    let seal_started = Instant::now();
    log_phase(
        "seal-start",
        count,
        count,
        seal_started.elapsed(),
        started.elapsed(),
    );
    fixture.seal();
    let seal_elapsed = seal_started.elapsed();
    log_phase(
        "seal-complete",
        count,
        count,
        seal_elapsed,
        started.elapsed(),
    );
    eprintln!(
        "[HEADER-CACHE-PREPARED] {}",
        serde_json::json!({
            "files": count,
            "fileCreationMs": file_creation.as_millis(),
            "catalogWriteMs": catalog_write.as_millis(),
            "sealMs": seal_elapsed.as_millis(),
            "totalMs": started.elapsed().as_millis()
        })
    );
    (root, fixture)
}

fn read_files(database: &std::path::Path, count: usize) -> WorkerResult {
    let started = Instant::now();
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let cache = FileHeaderReadCache::new(CASE_ID);
    let mut peak_entries = 0;
    let mut peak_rss = crate::runtime_resources::current_rss_mb();
    log_phase("read-start", 0, count, started.elapsed(), started.elapsed());
    let mut report_started = Instant::now();
    for index in 0..count {
        let id = FileEntryId(format!("header-{index:06}"));
        let actual = cache.read_file_header_by_id(&connection, &id, 8).unwrap();
        assert_eq!(actual, (index as u64).to_le_bytes());
        let entries = cache.descriptors.borrow().len();
        assert!(entries <= MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES);
        peak_entries = peak_entries.max(entries);
        if index % 256 == 0 {
            peak_rss = peak_rss.max(crate::runtime_resources::current_rss_mb());
        }
        if (index + 1) % 10_000 == 0 || index + 1 == count {
            log_phase(
                "read-progress",
                index + 1,
                count,
                report_started.elapsed(),
                started.elapsed(),
            );
            report_started = Instant::now();
        }
    }
    assert_eq!(
        cache
            .read_file_header_by_id(&connection, &FileEntryId("header-000000".into()), 8)
            .unwrap(),
        0u64.to_le_bytes()
    );
    let final_entries = cache.descriptors.borrow().len();
    log_phase(
        "read-complete",
        count,
        count,
        started.elapsed(),
        started.elapsed(),
    );
    WorkerResult {
        files_read: count,
        peak_entries,
        final_entries,
        sampled_peak_rss_mb: peak_rss,
    }
}

fn log_phase(
    phase: &str,
    completed: usize,
    total: usize,
    phase_elapsed: Duration,
    total_elapsed: Duration,
) {
    eprintln!(
        "[HEADER-CACHE-PHASE] {}",
        serde_json::json!({
            "phase": phase,
            "worker": format!("{:?}", std::thread::current().id()),
            "completedFiles": completed,
            "totalFiles": total,
            "phaseElapsedMs": phase_elapsed.as_millis(),
            "totalElapsedMs": total_elapsed.as_millis()
        })
    );
}

#[test]
#[ignore = "explicit 100K real-file header cache benchmark"]
fn header_read_cache_real_100k_files_remains_bounded() {
    let (_root, fixture) = prepare_files(100_000);
    let _store = persistence_source::detail::open_store(&fixture.connection)
        .unwrap()
        .unwrap();
    let result = read_files(&fixture.source_dir.join("source.db"), 100_000);
    assert_eq!(result.files_read, 100_000);
    assert_eq!(result.peak_entries, MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES);
    assert!(result.final_entries <= MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES);
    eprintln!(
        "[HEADER-CACHE] files={} workers=1 peakEntries={} sampledPeakRssMb={}",
        result.files_read, result.peak_entries, result.sampled_peak_rss_mb
    );
}

#[test]
#[ignore = "explicit real-file header cache worker matrix"]
fn header_read_cache_real_worker_matrix_remains_bounded() {
    let count = MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES + 257;
    let (_root, fixture) = prepare_files(count);
    let _store = persistence_source::detail::open_store(&fixture.connection)
        .unwrap()
        .unwrap();
    for workers in [1, 2, 4] {
        let database = fixture.source_dir.join("source.db");
        let results = std::thread::scope(|scope| {
            let handles = (0..workers)
                .map(|_| scope.spawn(|| read_files(&database, count)))
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        let mut total_peak_entries = 0;
        for result in &results {
            assert_eq!(result.files_read, count);
            assert_eq!(result.peak_entries, MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES);
            assert!(result.final_entries <= MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES);
            total_peak_entries += result.peak_entries;
        }
        assert!(total_peak_entries <= workers * MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES);
        eprintln!("[HEADER-CACHE] filesPerWorker={count} workers={workers} peakEntries={total_peak_entries} sampledPeakRssMb={}",
            results.iter().map(|result| result.sampled_peak_rss_mb).max().unwrap());
    }
}
