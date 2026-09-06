use super::*;
use std::path::Path;

#[test]
fn rejects_non_physical_drive_paths() {
    let error = LocalDiskReader::open(Path::new("C:/disk.raw")).expect_err("must reject");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn rejects_malformed_physical_drive_paths() {
    for value in [
        r"\\.\PhysicalDrive",
        r"\\.\PhysicalDrive-1",
        r"\\.\PhysicalDrive0x",
    ] {
        let error = LocalDiskReader::open(Path::new(value)).expect_err("must reject");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[test]
fn accepts_case_insensitive_and_slash_normalized_paths() {
    assert_eq!(
        parse_physical_drive_path(Path::new(r"\\.\physicaldrive12")),
        Some(12)
    );
    assert_eq!(
        parse_physical_drive_path(Path::new(r"\\./PhysicalDrive7")),
        Some(7)
    );
}

#[test]
fn rejects_disk_number_overflow() {
    assert_eq!(
        parse_physical_drive_path(Path::new(r"\\.\PhysicalDrive4294967296")),
        None
    );
}
