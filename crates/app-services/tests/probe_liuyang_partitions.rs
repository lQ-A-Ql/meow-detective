//! Diagnostic probe: list partitions of the liuyang E01 and report which of
//! them contain `Windows/System32/config/SYSTEM` (hive ambiguity check for
//! the browser preload suffix locators).

use app_services::datasource_service::detect_image_filesystem;
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::path::PathBuf;

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn probe_partitions_and_system_hives() {
    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LIUYANG_E01_FIXTURE");
    let mut image = E01Reader::open(&fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    eprintln!("partitions: {}", probe.partitions.len());
    for partition in &probe.partitions {
        eprintln!(
            "  P{} kind={} offset={} length={}",
            partition.index, partition.kind_label, partition.offset, partition.length
        );
    }
    for candidate in &probe.candidates {
        let boxed: Box<dyn EvidenceReader> =
            Box::new(E01Reader::open(&fixture).expect("reopen E01"));
        match candidate.kind {
            app_services::datasource_service::ImageFilesystemKind::Ntfs => {
                let fs = fs_ntfs::NtfsReader::open(boxed, candidate.offset).expect("open NTFS");
                let has_system = fs.open_file("Windows/System32/config/SYSTEM").is_ok();
                let has_chrome = fs
                    .open_file("Users/刘洋/AppData/Local/Google/Chrome/User Data/Local State")
                    .is_ok();
                eprintln!(
                    "  candidate offset={} kind=NTFS system_hive={} chrome_local_state={}",
                    candidate.offset, has_system, has_chrome
                );
            }
            other => {
                eprintln!("  candidate offset={} kind={other:?}", candidate.offset);
            }
        }
    }
}
