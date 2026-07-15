use std::time::Instant;

use super::{query, validation, write};

const BLUESTORE_SOURCE_DB_ENV: &str = "FORENSICS_BLUESTORE_SOURCE_DB_FIXTURE";
const MAX_QUERY_MS: u128 = 60_000;
const MAX_VALIDATION_MS: u128 = 90_000;
const MAX_WRITE_MS: u128 = 90_000;
const MAX_COMMIT_MS: u128 = 30_000;
const MAX_PEAK_RSS_MB: u64 = 512;

#[test]
#[ignore = "requires a private source.db with a complete BlueStore semantic snapshot"]
fn real_bluestore_semantic_phase_performance() {
    let source_path = std::env::var_os(BLUESTORE_SOURCE_DB_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("set {BLUESTORE_SOURCE_DB_ENV} before running this test"));
    assert!(
        source_path.is_file(),
        "BlueStore source.db fixture is missing"
    );
    let source = rusqlite::Connection::open_with_flags(
        &source_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("open read-only BlueStore source database");
    let inventory_id: String = source
        .query_row(
            "SELECT inventory_id FROM ceph_bluestore_semantic_scans",
            [],
            |row| row.get(0),
        )
        .expect("query semantic inventory");

    let query_started = Instant::now();
    let aggregate = query::find_aggregate(&source, &inventory_id)
        .expect("query semantic aggregate")
        .expect("semantic aggregate exists");
    let query = report_phase("query", query_started, &aggregate);

    let validation_started = Instant::now();
    validation::validate_replacement(&aggregate).expect("validate semantic aggregate");
    let validation = report_phase("validation", validation_started, &aggregate);

    let temp = tempfile::TempDir::new().expect("create semantic benchmark directory");
    let target_path = temp.path().join("source.db");
    let target = crate::open_or_create_source(&target_path).expect("create target source database");
    seed_control_plane(&target, &source_path);
    let transaction = target.unchecked_transaction().expect("start replacement");
    let write_started = Instant::now();
    write::replace_for_inventory_on(&transaction, &aggregate).expect("write semantic aggregate");
    let write = report_phase("write", write_started, &aggregate);

    let commit_started = Instant::now();
    transaction.commit().expect("commit semantic aggregate");
    let commit = report_phase("commit", commit_started, &aggregate);
    let persisted_digest: String = target
        .query_row(
            "SELECT semantic_sha256
             FROM ceph_bluestore_semantic_scans
             WHERE inventory_id = ?1",
            [&inventory_id],
            |row| row.get(0),
        )
        .expect("query persisted semantic digest");
    assert_eq!(persisted_digest, aggregate.scan.semantic_sha256);
    assert!(query.elapsed_ms <= MAX_QUERY_MS, "semantic query regressed");
    assert!(
        validation.elapsed_ms <= MAX_VALIDATION_MS,
        "semantic validation regressed"
    );
    assert!(write.elapsed_ms <= MAX_WRITE_MS, "semantic write regressed");
    assert!(
        commit.elapsed_ms <= MAX_COMMIT_MS,
        "semantic commit regressed"
    );
    let peak_rss_mb = [query, validation, write, commit]
        .into_iter()
        .map(|phase| phase.peak_rss_mb)
        .max()
        .unwrap_or_default();
    assert!(
        peak_rss_mb <= MAX_PEAK_RSS_MB,
        "semantic peak RSS exceeded {MAX_PEAK_RSS_MB}MB: {peak_rss_mb}MB"
    );
}

fn seed_control_plane(target: &rusqlite::Connection, source_path: &std::path::Path) {
    target
        .execute(
            "ATTACH DATABASE ?1 AS fixture",
            [source_path.to_string_lossy()],
        )
        .expect("attach source fixture");
    let result = target.execute_batch(
        "BEGIN IMMEDIATE;
         INSERT INTO data_sources SELECT * FROM fixture.data_sources;
         INSERT INTO ceph_osd_inventory SELECT * FROM fixture.ceph_osd_inventory;
         INSERT INTO ceph_bluefs_superblocks SELECT * FROM fixture.ceph_bluefs_superblocks;
         INSERT INTO ceph_bluefs_replays SELECT * FROM fixture.ceph_bluefs_replays;
         INSERT INTO ceph_rocksdb_manifests SELECT * FROM fixture.ceph_rocksdb_manifests;
         INSERT INTO ceph_rocksdb_column_families
             SELECT * FROM fixture.ceph_rocksdb_column_families;
         INSERT INTO ceph_rocksdb_latest_state
             SELECT * FROM fixture.ceph_rocksdb_latest_state;
         COMMIT;",
    );
    if result.is_err() {
        let _ = target.execute_batch("ROLLBACK");
    }
    target
        .execute_batch("DETACH DATABASE fixture")
        .expect("detach source fixture");
    result.expect("seed BlueStore control plane");
}

#[derive(Clone, Copy)]
struct PhaseMeasurement {
    elapsed_ms: u128,
    peak_rss_mb: u64,
}

fn report_phase(
    phase: &str,
    started: Instant,
    aggregate: &super::CephBluestoreSemanticAggregate,
) -> PhaseMeasurement {
    let elapsed_ms = started.elapsed().as_millis();
    let (rss_mb, peak_rss_mb) = process_memory_mb();
    eprintln!(
        "BLUESTORE_SEMANTIC_PHASE phase={} elapsed_ms={} checksum_rows={} rss_mb={} peak_rss_mb={}",
        phase,
        elapsed_ms,
        aggregate.checksum_chunks.len(),
        rss_mb,
        peak_rss_mb
    );
    PhaseMeasurement {
        elapsed_ms,
        peak_rss_mb,
    }
}

#[cfg(target_os = "windows")]
fn process_memory_mb() -> (u64, u64) {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }
    #[link(name = "Psapi")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }
    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    // SAFETY: The pseudo handle is valid for the current process and the
    // counters buffer has the exact layout and size required by Psapi.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 {
        (0, 0)
    } else {
        (
            counters.WorkingSetSize as u64 / (1024 * 1024),
            counters.PeakWorkingSetSize as u64 / (1024 * 1024),
        )
    }
}

#[cfg(not(target_os = "windows"))]
fn process_memory_mb() -> (u64, u64) {
    (0, 0)
}
