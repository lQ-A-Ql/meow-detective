use app_services::{
    artifact_service, case_service, correlation, datasource_service, timeline_service,
};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::io::Read;
use std::time::Instant;
use tempfile::TempDir;
#[path = "support/ntfs_catalog.rs"]
mod ntfs_catalog;
#[path = "support/source_connection.rs"]
mod source_connection;

fn sample_path() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_REAL_ARTIFACT_E01")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("set FORENSICS_REAL_ARTIFACT_E01 to run ignored artifact tests"))
}

// Local run:
//   $env:FORENSICS_REAL_ARTIFACT_E01='<path-to-private-windows-sample.E01>'
//   cargo test -p app-services --test real_artifact_extraction_test -- --ignored --nocapture
#[test]
#[ignore = "requires FORENSICS_REAL_ARTIFACT_E01 real E01 sample"]
fn real_e01_lnk_browser_prefetch_artifact_extraction() {
    let fixture_path = sample_path();
    let start = Instant::now();

    // ── Open E01, probe, find best NTFS partition ────────────────────────
    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    assert!(
        !probe.candidates.is_empty(),
        "No filesystem candidates found"
    );

    // Pick the first NTFS candidate (smart: prefer the one with largest offset,
    // which is typically the main Windows partition in multi-partition layouts).
    let ntfs = probe
        .candidates
        .iter()
        .filter(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .max_by_key(|c| c.offset)
        .expect("NTFS candidate required");
    eprintln!(
        "Selected NTFS candidate: offset={} partition_index={:?} name={:?}",
        ntfs.offset, ntfs.partition_index, ntfs.partition_name
    );

    // ── Create case, register data source, import MFT ────────────────────
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "real-artifact-test",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();
    let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());

    active
        .with_conn(|conn| {
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "real-e01-sample".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let source = source_connection::open(conn, &active, &data_source_id);
            let conn = &source;
            let stats = ntfs_catalog::enumerate(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                0,
            )?;
            eprintln!(
                "MFT import: files={} dirs={} in {:?}",
                stats.file_count,
                stats.dir_count,
                start.elapsed()
            );
            assert!(stats.file_count > 1000, "Should enumerate many files");

            let mft_elapsed = start.elapsed();
            eprintln!(
                "[BENCH-OUTPUT] scenario=mft_import dataset_level=large p95_ms={}",
                mft_elapsed.as_millis()
            );

            // ── Scan file entries for artifact patterns ─────────────────
            let repo = FileRepo::new(conn);
            let all_entries = repo.find_by_data_source(&data_source_id)?;

            let lnk: Vec<_> = all_entries
                .iter()
                .filter(|e| e.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("lnk")))
                .take(10)
                .cloned()
                .collect();

            let browser: Vec<_> = all_entries
                .iter()
                .filter(|e| {
                    let p = e.path.to_lowercase();
                    p.contains("history") || p.contains("places.sqlite")
                })
                .take(5)
                .cloned()
                .collect();

            let prefetch: Vec<_> = all_entries
                .iter()
                .filter(|e| e.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pf")))
                .take(10)
                .cloned()
                .collect();

            let recycle_bin: Vec<_> = all_entries
                .iter()
                .filter(|e| {
                    e.path.to_lowercase().contains("$recycle.bin")
                        && e.name.to_uppercase().starts_with("$I")
                })
                .take(10)
                .cloned()
                .collect();

            eprintln!(
                "Found artifact candidates: lnk={} browser={} prefetch={} recyclebin={}",
                lnk.len(),
                browser.len(),
                prefetch.len(),
                recycle_bin.len()
            );

            // ── Open E01 filesystem reader ──────────────────────────────
            let boxed: Box<dyn evidence_core::EvidenceReader> =
                Box::new(E01Reader::open(&fixture_path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();

            let registry = artifact_service::create_registry();
            let mut sink = artifacts_core::VecSink::new();
            let mut families_with_artifacts = std::collections::HashSet::new();

            // ── Extract LNK artifacts ───────────────────────────────────
            for entry in &lnk {
                match fs.open_file(&entry.path) {
                    Ok(mut reader) => {
                        let mut buf = Vec::new();
                        if reader.read_to_end(&mut buf).is_ok() {
                            let file_reader: Box<dyn Read> = Box::new(std::io::Cursor::new(buf));
                            let _ = artifact_service::run_extractors_on_file(
                                &registry,
                                &entry.id,
                                &entry.path,
                                file_reader,
                                &mut sink,
                            );
                            families_with_artifacts.insert("lnk".to_string());
                        }
                    }
                    Err(e) => eprintln!("  skip LNK {}: {e}", entry.path),
                }
            }

            // ── Extract Prefetch artifacts ──────────────────────────────
            for entry in &prefetch {
                match fs.open_file(&entry.path) {
                    Ok(mut reader) => {
                        let mut buf = Vec::new();
                        if reader.read_to_end(&mut buf).is_ok() {
                            let file_reader: Box<dyn Read> = Box::new(std::io::Cursor::new(buf));
                            let _ = artifact_service::run_extractors_on_file(
                                &registry,
                                &entry.id,
                                &entry.path,
                                file_reader,
                                &mut sink,
                            );
                            families_with_artifacts.insert("prefetch".to_string());
                        }
                    }
                    Err(e) => eprintln!("  skip Prefetch {}: {e}", entry.path),
                }
            }

            // ── Extract RecycleBin artifacts ────────────────────────────
            for entry in &recycle_bin {
                match fs.open_file(&entry.path) {
                    Ok(mut reader) => {
                        let mut buf = Vec::new();
                        if reader.read_to_end(&mut buf).is_ok() {
                            let file_reader: Box<dyn Read> = Box::new(std::io::Cursor::new(buf));
                            let _ = artifact_service::run_extractors_on_file(
                                &registry,
                                &entry.id,
                                &entry.path,
                                file_reader,
                                &mut sink,
                            );
                            families_with_artifacts.insert("recyclebin".to_string());
                        }
                    }
                    Err(e) => eprintln!("  skip RecycleBin {}: {e}", entry.path),
                }
            }

            // ── Store artifacts and build timeline ──────────────────────
            if !sink.artifacts.is_empty() {
                artifact_service::store_artifacts(
                    conn,
                    &sink.artifacts,
                    &case_id.0,
                    &data_source_id.0,
                )
                .unwrap();
                eprintln!(
                    "Stored {} artifacts; families extracted: {:?}",
                    sink.artifacts.len(),
                    families_with_artifacts
                );
            }

            timeline_service::materialize_file_activity_unknown(conn).ok();

            // ── Run correlation and assert per-family leads ─────────────
            let corr_start = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            eprintln!(
                "[BENCH-OUTPUT] scenario=correlation_snapshot dataset_level=large p95_ms={}",
                corr_start.elapsed().as_millis()
            );
            eprintln!(
                "Correlation: nodes={} edges={} clusters={} leads={}",
                snapshot.node_count, snapshot.edge_count,
                snapshot.cluster_count, snapshot.lead_count
            );

            // ── Assert each extracted family produces at least 1 lead ───
            let target_families = ["lnk", "prefetch", "recycle_bin"];
            let mut covered: std::collections::HashSet<&str> =
                std::collections::HashSet::new();

            for fc in &snapshot.family_coverage {
                let family_lower = fc.family.to_lowercase();
                eprintln!(
                    "  family {}: status={:?} leads={} high_conf={} review={} clusters={}",
                    fc.family, fc.status, fc.lead_count,
                    fc.high_confidence_lead_count, fc.review_lead_count,
                    fc.cluster_count
                );
                if target_families.iter().any(|t| family_lower.contains(t)) && fc.lead_count > 0 {
                    covered.insert(fc.family.as_str());
                }
            }

            // Assert: each target family with extracted artifacts must have >0 leads
            for target in &target_families {
                assert!(
                    snapshot.family_coverage.iter().any(|fc| {
                        fc.family.to_lowercase().contains(target) && fc.lead_count > 0
                    }),
                    "Artifact family '{target}' should produce at least 1 correlation lead after extraction"
                );
            }

            eprintln!(
                "Families with leads: {:?} ({}/{})",
                covered,
                covered.len(),
                target_families.len()
            );

            let total_elapsed = start.elapsed();
            eprintln!("=== Real artifact extraction test complete in {total_elapsed:?} ===");

            Ok(())
        })
        .unwrap();
}

// ── Helpers ───────────────────────────────────────────────────────────────
