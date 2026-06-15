use app_services::{
    artifact_service, case_service, correlation, datasource_service, file_service,
    timeline_service, v2_governance_service,
};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
};
/// Full V2/V3 pipeline for 检材2.E01 (MBR, 3 NTFS partitions)
/// Run: cargo test -p app-services --test jc2_pipeline -- --nocapture
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;
use tempfile::TempDir;

const SAMPLE_PATH: &str = "D:/獬豸杯/检材2.E01";
// MBR: partition_index=None → after fix: 0=offset-1MB, 1=offset-580MB(system), 2=offset-50.6GB
const MAIN_PARTITION_INDEX: usize = 1; // system drive with 69K files
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
        v if v < 0 => (1u32 << v.unsigned_abs()).max(512),
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
fn jc2_full_pipeline() {
    let path = Path::new(SAMPLE_PATH);
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
    assert!(candidates.len() == 3, "expected 3 NTFS candidates");

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

            // Extract Registry hives
            let hives = [
                ("SYSTEM", "Windows/System32/config/SYSTEM"),
                ("SOFTWARE", "Windows/System32/config/SOFTWARE"),
                ("SAM", "Windows/System32/config/SAM"),
                ("SECURITY", "Windows/System32/config/SECURITY"),
            ];
            let mut total_artifacts = 0u32;
            for (name, hive_path) in &hives {
                let mut buf = Vec::new();
                if fs
                    .open_file(hive_path)
                    .and_then(|mut f| f.read_to_end(&mut buf))
                    .is_ok()
                {
                    let mut sink = artifacts_core::VecSink::new();
                    let reader: Box<dyn std::io::Read> = Box::new(std::io::Cursor::new(buf));
                    if artifact_service::run_extractors_on_file(
                        &registry,
                        &domain::FileEntryId(format!("jc2-{name}")),
                        hive_path,
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
                        println!("  extracted {}: {} artifacts", name, sink.artifacts.len());
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
            println!(
                "[BENCH] artifact_extraction: {artifact_ms}ms, total artifacts={}",
                total_artifacts
            );

            // Phase 3: Timeline
            let t2 = Instant::now();
            timeline_service::ensure_macb_timeline_projected(conn).ok();
            let tl = timeline_service::query_timeline(conn, 0, 100).unwrap();
            let tl_ms = t2.elapsed().as_millis();
            println!(
                "[BENCH] timeline: {tl_ms}ms, items(first_page)={}",
                tl.items.len()
            );

            // Phase 4: Correlation
            let t3 = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            let corr_ms = t3.elapsed().as_millis();
            println!(
                "[BENCH] correlation: {corr_ms}ms, nodes={} edges={} clusters={} leads={}",
                snapshot.node_count,
                snapshot.edge_count,
                snapshot.cluster_count,
                snapshot.lead_count
            );
            let covered: Vec<_> = snapshot
                .family_coverage
                .iter()
                .filter(|fc| fc.lead_count > 0)
                .map(|fc| format!("{}={}leads({:?})", fc.family, fc.lead_count, fc.status))
                .collect();
            println!("  families with leads: {:?}", covered);

            // Phase 5: Governance
            let t4 = Instant::now();
            let gov = v2_governance_service::get_v2_governance_snapshot(conn, &case_id.0).unwrap();
            let gov_ms = t4.elapsed().as_millis();
            println!(
                "[BENCH] governance: {gov_ms}ms, score={}/100 grade={} gates={}/{}",
                gov.release_scorecard.total_score,
                gov.release_scorecard.grade,
                gov.release_gates
                    .iter()
                    .filter(|g| g.status == transport::dto::ReleaseGateStatusDto::Passed)
                    .count(),
                gov.release_gates.len()
            );

            let total_ms = start.elapsed().as_millis();
            println!("=== 检材2 full pipeline: {total_ms}ms ===");
            assert!(stats.file_count > 1000);
            assert!(total_artifacts > 0, "should extract some artifacts");
            assert!(snapshot.node_count > 0, "should have correlation nodes");
            Ok(())
        })
        .unwrap();
}
