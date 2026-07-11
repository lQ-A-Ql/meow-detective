use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::case_service;
use chrono::Utc;
use domain::DataSourceId;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use tempfile::TempDir;

use crate::commands::import::pipeline::{execute_import_job, ImportJobOptions};

fn prefetch_fixture(exe_name: &str, run_count: u32, last_run: chrono::DateTime<Utc>) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x1Eu32.to_le_bytes());
    data.extend_from_slice(b"SCCA");
    data.extend_from_slice(&0x11u32.to_le_bytes());
    data.extend_from_slice(&0x0000A000u32.to_le_bytes());

    let mut name_buffer = vec![0u8; 60];
    for (index, character) in exe_name.encode_utf16().enumerate() {
        let offset = index * 2;
        if offset + 1 < name_buffer.len() {
            name_buffer[offset] = (character & 0xFF) as u8;
            name_buffer[offset + 1] = (character >> 8) as u8;
        }
    }
    data.extend_from_slice(&name_buffer);
    data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let filetime = |timestamp: chrono::DateTime<Utc>| -> u64 {
        ((timestamp.timestamp() + 11_644_473_600) as u64 * 10_000_000)
            + (timestamp.timestamp_subsec_nanos() as u64 / 100)
    };
    let mut file_info = vec![0u8; 212];
    file_info[0..4].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[8..12].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[16..20].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[24..28].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[44..52].copy_from_slice(&filetime(last_run).to_le_bytes());
    file_info[116..120].copy_from_slice(&run_count.to_le_bytes());
    file_info[120..124].copy_from_slice(&1u32.to_le_bytes());
    file_info[124..128].copy_from_slice(&3u32.to_le_bytes());
    file_info[128..132].copy_from_slice(&0x128u32.to_le_bytes());
    data.extend_from_slice(&file_info);
    data.resize(4096, 0);
    data
}

pub(super) fn setup_case() -> (app_services::active_case::ActiveCase, TempDir) {
    let temporary = TempDir::new().unwrap();
    let evidence_dir = temporary.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(
        evidence_dir.join("notes.txt"),
        "Forensics import marker: fw_bench_search_a1b2c3",
    )
    .unwrap();
    std::fs::write(
        evidence_dir.join("system-log.txt"),
        "System log: boot at 2026-01-15, user alice logged in at 2026-01-15T08:00:00Z\n",
    )
    .unwrap();
    std::fs::write(
        evidence_dir.join("CMD.EXE-DEADBEEF.pf"),
        prefetch_fixture("CMD.EXE", 3, Utc::now()),
    )
    .unwrap();
    std::fs::write(
        evidence_dir.join("NOTEPAD.EXE-12345678.pf"),
        prefetch_fixture("NOTEPAD.EXE", 1, Utc::now()),
    )
    .unwrap();

    let active = case_service::create_case(
        &temporary.path().join("cases"),
        "bench-case",
        Some("benchmark-runner"),
    )
    .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    active
        .with_conn(|connection| {
            let job_id = JobRepo::new(connection)
                .create(&active.meta.id.0, "Benchmark import")
                .unwrap();
            let import_config =
                app_services::import_precheck::prepare_import_source_config_from_path(
                    &evidence_dir.to_string_lossy(),
                    domain::DataSourcePlatform::Windows,
                )
                .unwrap();
            execute_import_job(
                connection,
                &active.meta.id,
                &active.case_root,
                import_config,
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: None,
                    max_analysis_workers: Some(1),
                    analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
                },
            )
            .expect("benchmark setup import should succeed");
            Ok(())
        })
        .unwrap();
    (active, temporary)
}

pub(super) fn first_data_source_id(
    connection: &rusqlite::Connection,
    case_id: &domain::CaseId,
) -> DataSourceId {
    DataSourceRepo::new(connection)
        .find_by_case(case_id)
        .unwrap()
        .into_iter()
        .next()
        .expect("benchmark import should register a data source")
        .id
}
