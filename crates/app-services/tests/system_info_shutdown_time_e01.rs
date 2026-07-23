//! Verify extract_system_info_for_case surfaces the registry ShutdownTime as
//! the data source's last shutdown time on the liuyang sample.

use app_services::analysis_service::extract_system_info_for_case;
use app_services::datasource_service::detect_image_filesystem;
use domain::FileEntryId;
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::Read;
use std::path::PathBuf;

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_system_info_reports_final_shutdown_time() {
    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LIUYANG_E01_FIXTURE");
    let mut image = E01Reader::open(&fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("NTFS candidate");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture).expect("reopen E01"));
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).expect("open NTFS");
    let mut file = fs
        .open_file("Windows/System32/config/SYSTEM")
        .expect("open SYSTEM hive");
    let mut hive = Vec::new();
    file.read_to_end(&mut hive).expect("read SYSTEM hive");

    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(
        "CREATE TABLE file_entries(
            id TEXT PRIMARY KEY, parent_id TEXT, data_source_id TEXT, path TEXT, name TEXT,
            entry_type TEXT, size INTEGER, ext TEXT, deleted INTEGER, hidden INTEGER,
            system INTEGER, created_at TEXT, modified_at TEXT, accessed_at TEXT,
            changed_at TEXT, hash_sha256 TEXT
        );",
    )
    .expect("create file_entries");
    conn.execute(
        "INSERT INTO file_entries VALUES (
            'file:system', NULL, 'ds-1', '[P3]/Windows/System32/config/SYSTEM', 'SYSTEM',
            'file', 0, '', 0, 0, 0, '', '', '', '', ''
        )",
        [],
    )
    .expect("insert SYSTEM hive row");

    let mut reader =
        move |_id: &FileEntryId, _limit: usize| -> Result<Vec<u8>, String> { Ok(hive.clone()) };
    let info = extract_system_info_for_case(&conn, &mut reader);
    eprintln!("system info warnings: {:?}", info.warnings);
    assert_eq!(
        info.shutdown_time.as_deref(),
        Some("2026-04-20T16:25:35.045057200+00:00"),
        "system info must surface the registry ShutdownTime as the last shutdown"
    );
}
