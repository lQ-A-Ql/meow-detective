//! Diagnostic: read SYSTEM\Control\Windows\ShutdownTime from the liuyang E01.
//! If it matches the VM's event 13 timestamp (~16:25:35), it corroborates that
//! the shutdown time was persisted to the registry during late shutdown and
//! that the Kernel-General 13 log record is written from it at the next boot.

use app_services::datasource_service::detect_image_filesystem;
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::Read;
use std::path::PathBuf;

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn probe_registry_shutdown_time() {
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
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("read SYSTEM hive");

    let entries = artifacts_windows::extract_shutdown_time_from_system_hive(&bytes, "SYSTEM")
        .expect("extract shutdown time");
    for entry in &entries {
        eprintln!(
            "key_path={} shutdown_time={:?}",
            entry.key_path, entry.shutdown_time
        );
    }
    assert!(!entries.is_empty(), "ShutdownTime must be present");
}
