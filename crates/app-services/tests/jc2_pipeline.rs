//! Opt-in end-to-end regression for a private Windows E01 fixture.
//! Run with `FORENSICS_JC2_E01_FIXTURE` and `cargo test -p app-services --test
//! jc2_pipeline -- --ignored --nocapture`.

use app_services::{
    analysis_service::{extract_registry_candidate, EvidenceCandidate},
    artifact_service, case_service, correlation, datasource_service, file_service, import_analysis,
    search_service, timeline_service, v2_governance_service,
};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::TempDir;
use transport::commands::ExportScopeDto;

const SAMPLE_ENV: &str = "FORENSICS_JC2_E01_FIXTURE";

fn sample_path() -> PathBuf {
    std::env::var_os(SAMPLE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {SAMPLE_ENV} to run ignored real E01 pipeline tests"))
}
// MBR: partition_index=None → after fix: 0=offset-1MB, 1=offset-580MB(system), 2=offset-50.6GB
const MAIN_NTFS_OFFSET: u64 = 608_174_080;

fn read_mft_params(path: &Path, vol_offset: u64) -> (u64, u64, u32, u16, u64) {
    let mut r = E01Reader::open(path).unwrap();
    r.seek(SeekFrom::Start(vol_offset)).unwrap();
    let mut boot = [0u8; 512];
    r.read_exact(&mut boot).unwrap();
    let bps = u16::from_le_bytes([boot[11], boot[12]]);
    let cs = bps as u64 * boot[13] as u64;
    let mc = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());
    let rs = match boot[0x40] as i8 {
        v if v > 0 => 1024,
        v if v < 0 => {
            // boot[0x40] encodes the MFT record size as 2^(-v) when negative.
            // Guard against overflow in debug mode when v is unusually large.
            let shift = v.unsigned_abs();
            1u32.checked_shl(shift as u32).unwrap_or(4096).max(512)
        }
        _ => 1024,
    };
    let mft_off = vol_offset + mc * cs;
    r.seek(SeekFrom::Start(mft_off)).unwrap();
    let mut rec = vec![0u8; rs as usize];
    r.read_exact(&mut rec).unwrap();
    let mft_size = {
        let mut sz = 100 * 1024 * 1024u64;
        if &rec[0..4] == b"FILE" {
            let ao = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
            let mut p = ao;
            while p + 8 < rec.len() {
                let t = u32::from_le_bytes(rec[p..p + 4].try_into().unwrap());
                if t == 0xFFFF_FFFF {
                    break;
                }
                let l = u32::from_le_bytes(rec[p + 4..p + 8].try_into().unwrap()) as usize;
                if l < 4 || p + l > rec.len() {
                    break;
                }
                if t == 0x80 && p + 0x38 <= rec.len() && (rec[p + 8] & 1) != 0 {
                    sz = u64::from_le_bytes(rec[p + 0x30..p + 0x38].try_into().unwrap());
                    break;
                }
                p += l;
            }
        }
        sz
    };
    (mc, cs, rs, bps, mft_size)
}

#[test]
#[ignore = "requires FORENSICS_JC2_E01_FIXTURE real E01 sample"]
fn jc2_full_pipeline() {
    let sample = sample_path();
    let path = sample.as_path();
    let start = Instant::now();

    // Probe
    let mut reader = E01Reader::open(path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let candidates = probe.candidates;
    println!(
        "检材2 probe: {} candidates, {} partitions",
        candidates.len(),
        probe.partitions.len()
    );
    for (i, c) in candidates.iter().enumerate() {
        println!("  [{}] {:?} @ offset={}", i, c.kind, c.offset);
    }
    let ntfs_count = candidates
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .count();
    assert!(
        ntfs_count == 3,
        "expected 3 NTFS candidates, got {ntfs_count}"
    );

    // Open system partition
    let (mc, cs, rs, bps, mft_size) = read_mft_params(path, MAIN_NTFS_OFFSET);
    println!("MFT: cluster={mc} cs={cs} rs={rs} bps={bps} mft_size={mft_size}");

    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "jc2-pipeline", Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "jc2-system".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: path.to_path_buf(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Phase 1: Import MFT
            let t0 = Instant::now();
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &ds_id,
                path,
                MAIN_NTFS_OFFSET,
                mc,
                cs,
                rs,
                bps,
                mft_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            let import_ms = t0.elapsed().as_millis();
            println!(
                "[BENCH] import_mft: {import_ms}ms, files={}, dirs={}",
                stats.file_count, stats.dir_count
            );

            // Phase 2: Extract artifacts
            let t1 = Instant::now();
            let registry = artifact_service::create_registry();
            let boxed: Box<dyn evidence_core::EvidenceReader> =
                Box::new(E01Reader::open(path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, MAIN_NTFS_OFFSET).unwrap();

            // Extract Registry hives via canonical analysis_service lookup path.
            let hives = [
                ("SYSTEM", "Windows/System32/config/SYSTEM"),
                ("SOFTWARE", "Windows/System32/config/SOFTWARE"),
                ("SAM", "Windows/System32/config/SAM"),
                ("SECURITY", "Windows/System32/config/SECURITY"),
            ];
            let mut total_artifacts = 0u32;

            // Pre-load SYSTEM so SAM/SECURITY can reuse the BootKey.
            let mut system_bytes = Vec::new();
            if fs
                .open_file("Windows/System32/config/SYSTEM")
                .and_then(|mut f| f.read_to_end(&mut system_bytes))
                .is_ok()
            {
                let _ = &system_bytes;
            }
            let boot_key = artifacts_windows::extract_boot_key(&system_bytes);

            for (name, hive_path) in &hives {
                let mut buf = Vec::new();
                if fs
                    .open_file(hive_path)
                    .and_then(|mut f| f.read_to_end(&mut buf))
                    .is_ok()
                {
                    let candidate = EvidenceCandidate {
                        file_id: domain::FileEntryId(format!("jc2-{name}")),
                        data_source_id: ds_id.0.clone(),
                        partition_index: None,
                        path: hive_path.to_string(),
                        size: buf.len() as u64,
                        encrypted: false,
                        content_identity: format!("test:{name}"),
                        companions: Vec::new(),
                        modified_at: None,
                        evidence_kind: "registry_hive".to_string(),
                        parser: "registry.hive".to_string(),
                        category: "Registry".to_string(),
                    };
                    let outcome = extract_registry_candidate(
                        &candidate,
                        &buf,
                        boot_key,
                        None,
                        None,
                    );
                    if !outcome.artifacts.is_empty() {
                        artifact_service::store_artifacts(
                            conn,
                            &outcome.artifacts,
                            &case_id.0,
                            &ds_id.0,
                        )
                        .unwrap();
                        total_artifacts += outcome.artifacts.len() as u32;
                        println!("  extracted {}: {} artifacts", name, outcome.artifacts.len());
                    }
                }
            }

            // Extract EVTX
            for evtx_path in &[
                "Windows/System32/winevt/Logs/System.evtx",
                "Windows/System32/winevt/Logs/Application.evtx",
                "Windows/System32/winevt/Logs/Security.evtx",
            ] {
                let mut buf = Vec::new();
                if fs
                    .open_file(evtx_path)
                    .and_then(|mut f| f.read_to_end(&mut buf))
                    .is_ok()
                {
                    let mut sink = artifacts_core::VecSink::new();
                    let reader: Box<dyn std::io::Read> = Box::new(std::io::Cursor::new(buf));
                    if artifact_service::run_extractors_on_file(
                        &registry,
                        &domain::FileEntryId("jc2-events".into()),
                        evtx_path,
                        reader,
                        &mut sink,
                    )
                    .is_ok()
                        && !sink.artifacts.is_empty()
                    {
                        artifact_service::store_artifacts(
                            conn,
                            &sink.artifacts,
                            &case_id.0,
                            &ds_id.0,
                        )
                        .unwrap();
                        total_artifacts += sink.artifacts.len() as u32;
                        println!(
                            "  extracted EVTX {}: {} artifacts",
                            evtx_path,
                            sink.artifacts.len()
                        );
                    }
                }
            }

            let artifact_ms = t1.elapsed().as_millis();
            let rss_after_artifacts = app_services::import_analysis::current_rss_mb();
            println!(
                "[BENCH-OUTPUT] scenario=artifact_extract dataset_level=large p95_ms={artifact_ms} rss_mb={rss_after_artifacts} artifact_count={total_artifacts}"
            );

            // Phase 3: File tree expand (warm)
            let _warm_tree = file_service::get_file_tree_real(conn).ok();
            let t_tree = Instant::now();
            let tree = file_service::get_file_tree_real(conn).unwrap();
            let tree_ms = t_tree.elapsed().as_millis();
            println!(
                "[BENCH-OUTPUT] scenario=file_tree_expand dataset_level=large p95_ms={tree_ms} rss_mb={rss_after_artifacts} node_count={}",
                tree.len()
            );

            // Phase 4: File pagination
            let t_page = Instant::now();
            let repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
            let page_result = repo.find_children_page(&domain::FileEntryId("mft:1:5".to_string()), 0, 50);
            let page_ms = t_page.elapsed().as_millis();
            let page_count = page_result.as_ref().map(|p| p.len()).unwrap_or(0);
            println!(
                "[BENCH-OUTPUT] scenario=file_paginate dataset_level=large p95_ms={page_ms} rss_mb={rss_after_artifacts} row_count={page_count}"
            );

            // Phase 5: Timeline projection + filter
            let t_tl = Instant::now();
            timeline_service::materialize_file_activity_unknown(conn).ok();
            let tl = timeline_service::query_timeline(conn, 0, 100).unwrap();
            let tl_ms = t_tl.elapsed().as_millis();
            println!(
                "[BENCH-OUTPUT] scenario=timeline_filter dataset_level=large p95_ms={tl_ms} rss_mb={rss_after_artifacts} event_count={}",
                tl.items.len()
            );

            // Phase 6: Search query
            let t_search = Instant::now();
            let query = "Windows";
            let search_results = search_service::search_files_real(
                &tmp.path().join("indexes").join("tantivy"),
                query,
                0,
                20,
            );
            let search_ms = t_search.elapsed().as_millis();
            let rss_after_search = import_analysis::current_rss_mb();
            let hit_count = search_results.as_ref().map(|r| r.items.len()).unwrap_or(0);
            println!(
                "[BENCH-OUTPUT] scenario=search_query dataset_level=large p95_ms={search_ms} rss_mb={rss_after_search} hit_count={hit_count}"
            );

            // Phase 7: Correlation
            let t3 = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            let corr_ms = t3.elapsed().as_millis();
            println!(
                "[BENCH-OUTPUT] scenario=correlation_snapshot dataset_level=large p95_ms={corr_ms} rss_mb={rss_after_search} nodes={} edges={} leads={}",
                snapshot.node_count,
                snapshot.edge_count,
                snapshot.lead_count
            );
            let covered: Vec<_> = snapshot
                .family_coverage
                .iter()
                .filter(|fc| fc.lead_count > 0)
                .map(|fc| format!("{}={}leads({:?})", fc.family, fc.lead_count, fc.status))
                .collect();
            println!("  families with leads: {:?}", covered);

            // Phase 8: Report export
            let t_report = Instant::now();
            let report = app_services::report::generate_html_report(
                conn,
                &domain::CaseMeta {
                    id: case_id.clone(),
                    name: "jc2-bench".to_string(),
                    number: None,
                    examiner: Some("bench".to_string()),
                    notes: None,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                },
                &tmp.path().join("reports"),
                &ExportScopeDto::default(),
            );
            let report_ms = t_report.elapsed().as_millis();
            let rss_final = import_analysis::current_rss_mb();
            let report_size_kb = report.as_ref().map(|r| r.len() / 1024).unwrap_or(0);
            println!(
                "[BENCH-OUTPUT] scenario=report_export dataset_level=large p95_ms={report_ms} rss_mb={rss_final} report_size_kb={report_size_kb}"
            );

            // Phase 9: Governance
            let t4 = Instant::now();
            let gov = v2_governance_service::get_v2_governance_snapshot(conn, &case_id.0).unwrap();
            let gov_ms = t4.elapsed().as_millis();
            println!(
                "[BENCH-OUTPUT] scenario=governance_snapshot dataset_level=large p95_ms={gov_ms} rss_mb={rss_final} score={} grade={}",
                gov.release_scorecard.total_score,
                gov.release_scorecard.grade
            );

            // ── Summary ──
            let total_ms = start.elapsed().as_millis();
            println!("=== 检材2 benchmark complete: {total_ms}ms, rss_peak={rss_final}MB ===");
            assert!(stats.file_count > 1000);
            assert!(total_artifacts > 0, "should extract some artifacts");
            assert!(snapshot.node_count > 0, "should have correlation nodes");
            Ok(())
        })
        .unwrap();
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn read_mft_params_for_partition(path: &Path, vol_offset: u64) -> (u64, u64, u32, u16, u64) {
    let mut r = E01Reader::open(path).unwrap();
    r.seek(SeekFrom::Start(vol_offset)).unwrap();
    let mut boot = [0u8; 512];
    r.read_exact(&mut boot).unwrap();
    let bps = u16::from_le_bytes([boot[11], boot[12]]);
    let cs = bps as u64 * boot[13] as u64;
    let mc = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());
    let rs = match boot[0x40] as i8 {
        v if v > 0 => 1024,
        v if v < 0 => {
            // boot[0x40] encodes the MFT record size as 2^(-v) when negative.
            // Guard against overflow in debug mode when v is unusually large.
            let shift = v.unsigned_abs();
            1u32.checked_shl(shift as u32).unwrap_or(4096).max(512)
        }
        _ => 1024,
    };
    let mft_off = vol_offset + mc * cs;
    r.seek(SeekFrom::Start(mft_off)).unwrap();
    let mut rec = vec![0u8; rs as usize];
    r.read_exact(&mut rec).unwrap();
    let mft_size = {
        let mut sz = 100 * 1024 * 1024u64;
        if &rec[0..4] == b"FILE" {
            let ao = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
            let mut p = ao;
            while p + 8 < rec.len() {
                let t = u32::from_le_bytes(rec[p..p + 4].try_into().unwrap());
                if t == 0xFFFF_FFFF {
                    break;
                }
                let l = u32::from_le_bytes(rec[p + 4..p + 8].try_into().unwrap()) as usize;
                if l < 4 || p + l > rec.len() {
                    break;
                }
                if t == 0x80 && p + 0x38 <= rec.len() && (rec[p + 8] & 1) != 0 {
                    sz = u64::from_le_bytes(rec[p + 0x30..p + 0x38].try_into().unwrap());
                    break;
                }
                p += l;
            }
        }
        sz
    };
    (mc, cs, rs, bps, mft_size)
}

// ── jc2_visibility_and_partitions ────────────────────────────────────────────

#[test]
#[ignore = "requires FORENSICS_JC2_E01_FIXTURE real E01 sample"]
fn jc2_visibility_and_partitions() {
    let sample = sample_path();
    let path = sample.as_path();
    let start = Instant::now();

    // ── Probe ──────────────────────────────────────────────────────────────
    let mut reader = E01Reader::open(path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let candidates = &probe.candidates;

    println!(
        "检材2 visibility probe: {} candidates, {} partition-records",
        candidates.len(),
        probe.partitions.len()
    );
    for (i, c) in candidates.iter().enumerate() {
        println!(
            "  candidate[{}]: {:?} offset={} partition_index={:?} source={:?} name={:?}",
            i, c.kind, c.offset, c.partition_index, c.source, c.partition_name
        );
    }

    // Verify 3 NTFS candidates (MBR layout)
    let ntfs_count = candidates
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .count();
    assert!(
        ntfs_count == 3,
        "expected 3 NTFS candidates in jc2 MBR layout, got {ntfs_count}"
    );

    // MBR probe now also produces PartitionRecord entries for non-empty, non-extended partitions
    assert_eq!(
        probe.partitions.len(),
        4,
        "MBR probe should have 4 PartitionRecord entries (3 NTFS + 1 BitLocker), got {}",
        probe.partitions.len()
    );

    // MBR candidates carry partition_index from MBR entry partition_number (0-indexed)
    for (i, c) in candidates.iter().enumerate() {
        assert_eq!(
            c.source,
            app_services::datasource_service::ImageFilesystemSource::MbrPartition,
            "candidate[{i}] should be MbrPartition source"
        );
        assert!(
            c.partition_index.is_some(),
            "candidate[{i}] partition_index should be Some for MBR"
        );
    }

    // ── Create case ────────────────────────────────────────────────────────
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "jc2-visibility", Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            // ── Import each partition separately and verify file counts ────
            let mut partition_stats = Vec::new();

            for (i, candidate) in candidates
                .iter()
                .filter(|c| {
                    matches!(
                        c.kind,
                        app_services::datasource_service::ImageFilesystemKind::Ntfs
                    )
                })
                .enumerate()
            {
                let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
                let ds_name = format!("jc2-part{}", candidate.partition_index.map_or(i, |idx| idx));

                DataSourceRepo::new(conn).insert(
                    &case_id,
                    &domain::DataSource {
                        id: ds_id.clone(),
                        name: ds_name.clone(),
                        kind: domain::DataSourceKind::E01,
                        source_path: path.to_path_buf(),
                        imported_at: chrono::Utc::now(),
                        provenance: domain::DataSourceProvenance::unknown(),
                    },
                )?;

                let (mc, cs, rs, bps, mft_size) =
                    read_mft_params_for_partition(path, candidate.offset);

                let t0 = Instant::now();
                let stats = file_service::enumerate_filesystem_mft(
                    conn,
                    &ds_id,
                    path,
                    candidate.offset,
                    mc,
                    cs,
                    rs,
                    bps,
                    mft_size,
                    Some(&|pct, msg| eprintln!("  [part{} MFT {pct}%] {msg}", i)),
                    None,
                )?;
                let ms = t0.elapsed().as_millis();
                println!(
                    "  partition {} (offset={}): files={} dirs={} in {ms}ms",
                    i, candidate.offset, stats.file_count, stats.dir_count
                );

                partition_stats.push((
                    i,
                    candidate.offset,
                    candidate.partition_index,
                    stats.file_count,
                    stats.dir_count,
                ));
            }

            // ── Verify per-partition expectations ─────────────────────────
            assert_eq!(partition_stats.len(), 3);

            // Partition 0: ~1 MB offset (recovery), few files
            let (p0_idx, p0_offset, _p0_part_idx, p0_files, p0_dirs) = &partition_stats[0];
            assert_eq!(*p0_idx, 0);
            assert!(
                *p0_offset < 2_000_000,
                "partition 0 should be at ~1 MB offset, got {}",
                p0_offset
            );
            assert!(
                *p0_files < 1000,
                "recovery partition should have few files, got {}",
                p0_files
            );
            println!(
                "  [PASS] partition 0 (recovery): {} files, {} dirs at offset {}",
                p0_files, p0_dirs, p0_offset
            );

            // Partition 1: ~580 MB offset (main system drive), ~69K files
            let (p1_idx, p1_offset, _p1_part_idx, p1_files, p1_dirs) = &partition_stats[1];
            assert_eq!(*p1_idx, 1);
            assert!(
                *p1_offset > 500_000_000 && *p1_offset < 700_000_000,
                "partition 1 (system) should be at ~580 MB offset, got {}",
                p1_offset
            );
            assert!(
                *p1_files > 50_000,
                "main system partition should have many files (>50K), got {}",
                p1_files
            );
            println!(
                "  [PASS] partition 1 (system): {} files, {} dirs at offset {}",
                p1_files, p1_dirs, p1_offset
            );

            // Partition 2: ~50.6 GB offset (data drive), has files
            let (p2_idx, p2_offset, _p2_part_idx, p2_files, p2_dirs) = &partition_stats[2];
            assert_eq!(*p2_idx, 2);
            assert!(
                *p2_offset > 50_000_000_000,
                "partition 2 (data) should be at ~50.6 GB offset, got {}",
                p2_offset
            );
            assert!(
                *p2_files > 0,
                "data partition should have some files, got {}",
                p2_files
            );
            println!(
                "  [PASS] partition 2 (data): {} files, {} dirs at offset {}",
                p2_files, p2_dirs, p2_offset
            );

            let total_ms = start.elapsed().as_millis();
            println!("=== jc2 visibility + partitions: {total_ms}ms ===");
            Ok(())
        })
        .unwrap();
}

// ── jc2_artifact_extraction ─────────────────────────────────────────────────

#[test]
#[ignore = "requires FORENSICS_JC2_E01_FIXTURE real E01 sample"]
fn jc2_artifact_extraction() {
    let sample = sample_path();
    let path = sample.as_path();
    let start = Instant::now();

    // ── Probe and find system partition (index 1 = main system drive) ──────
    let mut reader = E01Reader::open(path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs_candidates: Vec<_> = probe
        .candidates
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .collect();
    assert!(
        ntfs_candidates.len() >= 3,
        "need at least 3 NTFS partitions"
    );

    // Identify the system partition (index 1 = main system drive at ~580 MB offset)
    let system_idx = 1usize;
    let system_offset = ntfs_candidates[system_idx].offset;
    println!(
        "System partition: offset={} partition_index={:?}",
        system_offset, ntfs_candidates[system_idx].partition_index
    );

    // ── Create case ────────────────────────────────────────────────────────
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "jc2-artifacts", Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            // ── Import MFT for ALL NTFS partitions ─────────────────────────
            let mut ds_ids: Vec<domain::DataSourceId> = Vec::new();
            let mut system_files = 0u64;
            for (i, candidate) in ntfs_candidates.iter().enumerate() {
                let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
                DataSourceRepo::new(conn).insert(
                    &case_id,
                    &domain::DataSource {
                        id: ds_id.clone(),
                        name: format!("jc2-part{i}"),
                        kind: domain::DataSourceKind::E01,
                        source_path: path.to_path_buf(),
                        imported_at: chrono::Utc::now(),
                        provenance: domain::DataSourceProvenance::unknown(),
                    },
                )?;
                let (mc, cs, rs, bps, mft_size) =
                    read_mft_params_for_partition(path, candidate.offset);
                if i == system_idx {
                    println!("MFT params: cluster={mc} cs={cs} rs={rs} bps={bps} mft_size={mft_size}");
                }
                let t0 = Instant::now();
                let stats = file_service::enumerate_filesystem_mft(
                    conn,
                    &ds_id,
                    path,
                    candidate.offset,
                    mc,
                    cs,
                    rs,
                    bps,
                    mft_size,
                    Some(&|pct, msg| eprintln!("[part{i} MFT {pct}%] {msg}")),
                    None,
                )?;
                println!(
                    "MFT import part{i}: files={} dirs={} in {}ms (offset={})",
                    stats.file_count,
                    stats.dir_count,
                    t0.elapsed().as_millis(),
                    candidate.offset,
                );
                if i == system_idx {
                    system_files = stats.file_count;
                }
                ds_ids.push(ds_id);
            }
            assert!(system_files > 50_000, "system partition should be large");

            // Alias system ds_id for registry/EVTX extraction below
            let ds_id = domain::DataSourceId(ds_ids[system_idx].0.clone());

            // ── Open system filesystem for registry/EVTX ───────────────────
            let boxed: Box<dyn evidence_core::EvidenceReader> =
                Box::new(E01Reader::open(path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, system_offset).unwrap();

            let mut total_artifacts = 0u32;

            // ── Extract Registry hives ────────────────────────────────────
            let hives = [
                ("SYSTEM", "Windows/System32/config/SYSTEM"),
                ("SOFTWARE", "Windows/System32/config/SOFTWARE"),
                ("SAM", "Windows/System32/config/SAM"),
                ("SECURITY", "Windows/System32/config/SECURITY"),
            ];

            // Pre-load SYSTEM so SAM/SECURITY can reuse the BootKey.
            let mut system_bytes = Vec::new();
            if fs
                .open_file("Windows/System32/config/SYSTEM")
                .and_then(|mut f| f.read_to_end(&mut system_bytes))
                .is_ok()
            {
                let _ = &system_bytes;
            }
            let boot_key = artifacts_windows::extract_boot_key(&system_bytes);

            for (name, hive_path) in &hives {
                let mut buf = Vec::new();
                if fs
                    .open_file(hive_path)
                    .and_then(|mut f| f.read_to_end(&mut buf))
                    .is_ok()
                {
                    let candidate = EvidenceCandidate {
                        file_id: domain::FileEntryId(format!("jc2-art-{name}")),
                        data_source_id: ds_id.0.clone(),
                        partition_index: None,
                        path: hive_path.to_string(),
                        size: buf.len() as u64,
                        encrypted: false,
                        content_identity: format!("test:{name}"),
                        companions: Vec::new(),
                        modified_at: None,
                        evidence_kind: "registry_hive".to_string(),
                        parser: "registry.hive".to_string(),
                        category: "Registry".to_string(),
                    };
                    let outcome = extract_registry_candidate(
                        &candidate,
                        &buf,
                        boot_key,
                        None,
                        None,
                    );
                    if !outcome.artifacts.is_empty() {
                        artifact_service::store_artifacts(
                            conn,
                            &outcome.artifacts,
                            &case_id.0,
                            &ds_id.0,
                        )
                        .unwrap();
                        total_artifacts += outcome.artifacts.len() as u32;
                        println!("  registry {name}: {} artifacts", outcome.artifacts.len());
                    }
                }
            }

            // ── Extract EVTX boot/shutdown events ─────────────────────────
            let evtx_paths = [
                "Windows/System32/winevt/Logs/System.evtx",
                "Windows/System32/winevt/Logs/Application.evtx",
                "Windows/System32/winevt/Logs/Security.evtx",
            ];
            let mut evtx_artifacts: Vec<domain::Artifact> = Vec::new();
            for evtx_path in &evtx_paths {
                let mut buf = Vec::new();
                if fs
                    .open_file(evtx_path)
                    .and_then(|mut f| f.read_to_end(&mut buf))
                    .is_ok()
                {
                    if let Ok(extraction) =
                        artifacts_windows::extract_boot_shutdown_events(&buf, evtx_path)
                    {
                        println!(
                            "  EVTX {}: {} events, {} warnings",
                            evtx_path,
                            extraction.events.len(),
                            extraction.warnings.len()
                        );
                        for event in &extraction.events {
                        let mut attrs = BTreeMap::new();
                        attrs.insert(
                            "eventId".to_string(),
                            serde_json::Value::Number(event.event_id.into()),
                        );
                        attrs.insert(
                            "eventKind".to_string(),
                            serde_json::Value::String(event.kind.as_str().to_string()),
                        );
                        attrs.insert(
                            "provider".to_string(),
                            serde_json::Value::String(
                                event.provider.clone().unwrap_or_default(),
                            ),
                        );
                        attrs.insert(
                            "sourcePath".to_string(),
                            serde_json::Value::String(event.source_path.clone()),
                        );
                        attrs.insert(
                            "note".to_string(),
                            serde_json::Value::String(event.note.clone()),
                        );
                        evtx_artifacts.push(domain::Artifact {
                            id: domain::ArtifactId(uuid::Uuid::new_v4().to_string()),
                            family: "EvtxBootShutdown".to_string(),
                            title: format!(
                                "EVTX {} event {} ({})",
                                evtx_path, event.event_id, event.kind.as_str()
                            ),
                            summary: event.note.clone(),
                            source_object_id: Some(domain::FileEntryId(format!(
                                "jc2-evtx-{}",
                                event.event_id
                            ))),
                            extractor_id: Some("evtx.boot_shutdown".to_string()),
                            extractor_version: Some("1.0".to_string()),
                            confidence: Some(0.9),
                            source_attribution: Some(evtx_path.to_string()),
                            created_at: chrono::Utc::now(),
                            attrs,
                        });
                    }
                }
                }
            }
            if !evtx_artifacts.is_empty() {
                artifact_service::store_artifacts(
                    conn,
                    &evtx_artifacts,
                    &case_id.0,
                    &ds_id.0,
                )
                .unwrap();
                total_artifacts += evtx_artifacts.len() as u32;
                println!("  stored {} EVTX boot/shutdown artifacts", evtx_artifacts.len());
            }

            // ── Extract Browser history artifacts across ALL partitions ───
            let mut browser_artifacts: Vec<domain::Artifact> = Vec::new();

            // Scan each NTFS partition for user browser databases
            for (part_idx, candidate) in ntfs_candidates.iter().enumerate() {
                let part_fs = match fs_ntfs::NtfsReader::open(
                    Box::new(E01Reader::open(path).unwrap()) as Box<dyn evidence_core::EvidenceReader>,
                    candidate.offset,
                ) {
                    Ok(f) => f,
                    Err(_) => continue, // skip non-NTFS or unreadable partitions
                };
                let users_children = match part_fs.list_children("Users") {
                    Ok(u) => u,
                    Err(_) => continue, // no Users directory on this partition
                };
                println!(
                    "  scanning partition {part_idx} (offset={}) for browser databases",
                    candidate.offset
                );

                for user_entry in &users_children {
                    if !user_entry.is_dir
                        || user_entry.name == "Public"
                        || user_entry.name == "Default"
                    {
                        continue;
                    }

                    // Chrome History
                    let chrome_history = format!(
                        "{}/AppData/Local/Google/Chrome/User Data/Default/History",
                        user_entry.path
                    );

                    let mut buf = Vec::new();
                    if part_fs
                        .open_file(&chrome_history)
                        .and_then(|mut f| f.read_to_end(&mut buf))
                        .is_ok()
                    {
                        if let Ok(visits) =
                            artifacts_windows::parse_chrome_history(&buf, "Chrome", Some("Default"))
                        {
                            for visit in &visits {
                                let mut attrs = BTreeMap::new();
                                attrs.insert(
                                    "url".to_string(),
                                    serde_json::Value::String(visit.url.clone()),
                                );
                                attrs.insert(
                                    "title".to_string(),
                                    serde_json::Value::String(
                                        visit.title.clone().unwrap_or_default(),
                                    ),
                                );
                                attrs.insert(
                                    "browser".to_string(),
                                    serde_json::Value::String(visit.browser.clone()),
                                );
                                attrs.insert(
                                    "visitCount".to_string(),
                                    serde_json::Value::Number(
                                        (visit.visit_count as u32).into(),
                                    ),
                                );
                                if let Some(ref ts) = visit.visit_time {
                                    attrs.insert(
                                        "visitTime".to_string(),
                                        serde_json::Value::String(ts.to_rfc3339()),
                                    );
                                }
                                browser_artifacts.push(domain::Artifact {
                                    id: domain::ArtifactId(uuid::Uuid::new_v4().to_string()),
                                    family: "BrowserHistory".to_string(),
                                    title: format!(
                                        "Chrome visit: {}",
                                        visit.title.as_deref().unwrap_or("untitled")
                                    ),
                                    summary: format!(
                                        "{} ({} visits)",
                                        visit.url, visit.visit_count
                                    ),
                                    source_object_id: Some(domain::FileEntryId(
                                        format!("jc2-part{part_idx}-browser-chrome"),
                                    )),
                                    extractor_id: Some("browser.chromium.history".to_string()),
                                    extractor_version: Some("1.0".to_string()),
                                    confidence: Some(0.85),
                                    source_attribution: Some(chrome_history.clone()),
                                    created_at: chrono::Utc::now(),
                                    attrs,
                                });
                            }
                            if !visits.is_empty() {
                                println!(
                                    "  Chrome history {}: {} visits",
                                    chrome_history,
                                    visits.len()
                                );
                            }
                        }
                    }

                    // Edge History
                    let edge_history = format!(
                        "{}/AppData/Local/Microsoft/Edge/User Data/Default/History",
                        user_entry.path
                    );
                    buf.clear();
                    if part_fs
                        .open_file(&edge_history)
                        .and_then(|mut f| f.read_to_end(&mut buf))
                        .is_ok()
                    {
                        if let Ok(visits) =
                            artifacts_windows::parse_chrome_history(&buf, "Edge", Some("Default"))
                        {
                            for visit in &visits {
                                let mut attrs = BTreeMap::new();
                                attrs.insert(
                                    "url".to_string(),
                                    serde_json::Value::String(visit.url.clone()),
                                );
                                attrs.insert(
                                    "title".to_string(),
                                    serde_json::Value::String(
                                        visit.title.clone().unwrap_or_default(),
                                    ),
                                );
                                attrs.insert(
                                    "browser".to_string(),
                                    serde_json::Value::String(visit.browser.clone()),
                                );
                                attrs.insert(
                                    "visitCount".to_string(),
                                    serde_json::Value::Number(
                                        (visit.visit_count as u32).into(),
                                    ),
                                );
                                if let Some(ref ts) = visit.visit_time {
                                    attrs.insert(
                                        "visitTime".to_string(),
                                        serde_json::Value::String(ts.to_rfc3339()),
                                    );
                                }
                                browser_artifacts.push(domain::Artifact {
                                    id: domain::ArtifactId(uuid::Uuid::new_v4().to_string()),
                                    family: "BrowserHistory".to_string(),
                                    title: format!(
                                        "Edge visit: {}",
                                        visit.title.as_deref().unwrap_or("untitled")
                                    ),
                                    summary: format!(
                                        "{} ({} visits)",
                                        visit.url, visit.visit_count
                                    ),
                                    source_object_id: Some(domain::FileEntryId(
                                        format!("jc2-part{part_idx}-browser-edge"),
                                    )),
                                    extractor_id: Some("browser.chromium.history".to_string()),
                                    extractor_version: Some("1.0".to_string()),
                                    confidence: Some(0.85),
                                    source_attribution: Some(edge_history.clone()),
                                    created_at: chrono::Utc::now(),
                                    attrs,
                                });
                            }
                            if !visits.is_empty() {
                                println!(
                                    "  Edge history {}: {} visits",
                                    edge_history,
                                    visits.len()
                                );
                            }
                        }
                    }

                    // Early limit to avoid excessive memory
                    if browser_artifacts.len() > 5000 {
                        break;
                    }
                }
                if browser_artifacts.len() > 5000 {
                    break;
                }
            }

            if !browser_artifacts.is_empty() {
                artifact_service::store_artifacts(
                    conn,
                    &browser_artifacts,
                    &case_id.0,
                    &ds_id.0,
                )
                .unwrap();
                total_artifacts += browser_artifacts.len() as u32;
                println!(
                    "  stored {} browser history artifacts",
                    browser_artifacts.len()
                );
            } else {
                println!("  no browser history artifacts found (may not be a user workstation)");
            }

            println!("Total artifacts extracted: {total_artifacts}");
            assert!(total_artifacts > 0, "should extract some artifacts");

            // ── Timeline ──────────────────────────────────────────────────
            let t_tl = Instant::now();
            timeline_service::materialize_file_activity_unknown(conn).ok();
            let tl = timeline_service::query_timeline(conn, 0, 100).unwrap();
            println!(
                "Timeline: {} items in {}ms",
                tl.items.len(),
                t_tl.elapsed().as_millis()
            );

            // ── Correlation ───────────────────────────────────────────────
            let t_corr = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            println!(
                "Correlation: nodes={} edges={} clusters={} leads={} in {}ms",
                snapshot.node_count,
                snapshot.edge_count,
                snapshot.cluster_count,
                snapshot.lead_count,
                t_corr.elapsed().as_millis()
            );

            // ── Verify family-rule leads ──────────────────────────────────
            // After extracting Registry + EVTX + Browser artifacts,
            // correlation should produce leads for at least some families.
            for fc in &snapshot.family_coverage {
                println!(
                    "  family {}: status={:?} leads={} high_conf={} review={} clusters={} samples={:?}",
                    fc.family,
                    fc.status,
                    fc.lead_count,
                    fc.high_confidence_lead_count,
                    fc.review_lead_count,
                    fc.cluster_count,
                    fc.sample_signals
                );
            }

            let covered_families: Vec<_> = snapshot
                .family_coverage
                .iter()
                .filter(|fc| fc.lead_count > 0)
                .map(|fc| fc.family.as_str())
                .collect();
            println!(
                "Families with leads: {:?} ({}/{})",
                covered_families,
                covered_families.len(),
                snapshot.family_coverage.len()
            );

            assert!(
                snapshot.lead_count > 0,
                "correlation should produce at least one lead after artifact extraction"
            );
            assert!(
                !covered_families.is_empty(),
                "at least one artifact family should produce correlation leads"
            );

            let total_ms = start.elapsed().as_millis();
            println!(
                "=== jc2 artifact extraction + correlation: {total_ms}ms ==="
            );
            Ok(())
        })
        .unwrap();
}
