//! Explicit read-only private sample checks; no default path or case migration.
use std::{path::PathBuf, time::Instant};

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE direct NTFS E01"]
fn direct_ntfs_e01_probe_reports_a_nonzero_bounded_extent() {
    let path = std::env::var_os("FORENSICS_E01_FIXTURE").expect("explicit fixture required");
    let started = Instant::now();
    let mut reader = image_e01::E01Reader::open(std::path::Path::new(&path)).unwrap();
    let probe = app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    assert_eq!(probe.candidates.len(), 1);
    assert_eq!(
        probe.candidates[0].source,
        app_services::datasource_service::ImageFilesystemSource::DirectVolume
    );
    assert_eq!(probe.partitions[0].offset, 0);
    assert!(probe.partitions[0].length > 0);
    assert_eq!(probe.candidates[0].length, Some(probe.partitions[0].length));
    println!(
        "direct NTFS probeMs={} extentBytes={}",
        started.elapsed().as_millis(),
        probe.partitions[0].length
    );
}

#[test]
#[ignore = "requires explicit FORENSICS_CASE_ROOT, FORENSICS_CASE_ID and FORENSICS_DATA_SOURCE_ID"]
fn existing_case_direct_volume_preflight_is_read_only_and_explicit() {
    let root = PathBuf::from(
        std::env::var_os("FORENSICS_CASE_ROOT").expect("explicit case root required"),
    );
    let case =
        domain::CaseId(std::env::var("FORENSICS_CASE_ID").expect("explicit case ID required"));
    let source = domain::DataSourceId(
        std::env::var("FORENSICS_DATA_SOURCE_ID").expect("explicit source ID required"),
    );
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    conn.pragma_update(None, "query_only", true).unwrap();
    let started = Instant::now();
    let preflight =
        app_services::mount_service::emulation_preflight(&conn, &root, &case, &source).unwrap();
    let geometry = preflight.direct_volume.unwrap();
    assert!(geometry.declared_volume_bytes > 0);
    assert!(geometry.verified_extent_bytes.is_some());
    assert!(matches!(
        app_services::mount_service::validate_emulation_disk_layout(&conn, &case, &source),
        Err(app_services::mount_service::MountServiceError::DirectVolumeBootLayoutRequired)
    ));
    println!(
        "read-only preflightMs={} mediaBytes={} declaredBytes={} verifiedBytes={:?} serial={}",
        started.elapsed().as_millis(),
        geometry.logical_media_bytes,
        geometry.declared_volume_bytes,
        geometry.verified_extent_bytes,
        geometry.volume_serial
    );
}
