use app_services::{
    artifact_service, case_service, correlation, datasource_service, file_service, timeline_service,
};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;
use tempfile::TempDir;

fn sample_path() -> std::path::PathBuf {
    std::env::var("FORENSICS_REAL_ARTIFACT_E01")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("E:/pangushi/刘洋/liuyang_pc.E01"))
}

// Local run:
//   $env:FORENSICS_REAL_ARTIFACT_E01='E:/pangushi/刘洋/liuyang_pc.E01'
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

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

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

            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
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

            timeline_service::ensure_macb_timeline_projected(conn).ok();

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

fn read_mft_parameters(
    path: &std::path::Path,
    volume_offset: u64,
) -> std::io::Result<(u64, u64, u32, u16, u64)> {
    let mut reader = E01Reader::open(path)?;
    reader.seek(SeekFrom::Start(volume_offset))?;

    let mut boot = [0u8; 512];
    reader.read_exact(&mut boot)?;

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());
    let record_size = mft_record_size_from_boot(&boot);

    let mft_abs_offset = volume_offset + mft_cluster * cluster_size;
    reader.seek(SeekFrom::Start(mft_abs_offset))?;
    let mut mft_record = vec![0u8; record_size as usize];
    reader.read_exact(&mut mft_record)?;
    let mft_data_size = parse_mft_data_size(&mft_record).unwrap_or(100 * 1024 * 1024);

    Ok((
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
    ))
}

fn mft_record_size_from_boot(boot: &[u8]) -> u32 {
    let raw = boot[0x40] as i8;
    if raw > 0 {
        1024
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        if shift < 32 {
            (1u32 << shift).max(512)
        } else {
            1024
        }
    } else {
        1024
    }
}

fn parse_mft_data_size(record: &[u8]) -> Option<u64> {
    if record.len() < 4 || &record[0..4] != b"FILE" {
        return None;
    }
    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().ok()?);
        if typ == 0xFFFF_FFFF {
            break;
        }
        let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().ok()?) as usize;
        if len < 4 || pos + len > record.len() {
            break;
        }
        if typ == 0x80 && pos + 0x38 <= record.len() && (record[pos + 8] & 1) != 0 {
            return Some(u64::from_le_bytes(
                record[pos + 0x30..pos + 0x38].try_into().ok()?,
            ));
        }
        pos += len;
    }
    None
}
