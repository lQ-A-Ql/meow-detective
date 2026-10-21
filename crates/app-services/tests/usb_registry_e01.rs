//! Opt-in USB parser -> artifact persistence -> USB-tab summary regression.
use app_services::analysis_service::{self, EvidenceCandidate};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo,
};
use std::{io::Read, path::PathBuf};

#[test]
#[ignore = "requires FORENSICS_USB_E01_FIXTURE private Windows E01 with USB storage history"]
fn usb_storage_history_survives_extraction_and_summary() {
    let image =
        PathBuf::from(std::env::var_os("FORENSICS_USB_E01_FIXTURE").expect("explicit fixture"));
    let before = std::fs::metadata(&image).unwrap();
    let mut reader = E01Reader::open(&image).unwrap();
    let probe = app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let fs = probe
        .candidates
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .find_map(|candidate| {
            let fs = fs_ntfs::NtfsReader::open(
                Box::new(E01Reader::open(&image).unwrap()),
                candidate.offset,
            )
            .ok()?;
            fs.open_file("Windows/System32/config/SYSTEM").ok()?;
            Some(fs)
        })
        .expect("Windows SYSTEM filesystem");
    let mut bytes = Vec::new();
    fs.open_file("Windows/System32/config/SYSTEM")
        .unwrap()
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 128 * 1024 * 1024);

    let candidate = EvidenceCandidate {
        file_id: domain::FileEntryId("usb-system".into()),
        data_source_id: "usb-source".into(),
        partition_index: None,
        path: "Windows/System32/config/SYSTEM".into(),
        size: bytes.len() as u64,
        encrypted: false,
        content_identity: "usb-system".into(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "registry_hive".into(),
        parser: "registry.hive".into(),
        category: "Registry".into(),
    };
    let outcome =
        analysis_service::extract_registry_candidate(&candidate, &bytes, None, None, None);
    let artifacts: Vec<_> = outcome
        .artifacts
        .into_iter()
        .filter(|a| a.family == "RegistryUsbDevice")
        .collect();
    assert!(!artifacts.is_empty(), "USB storage history must be found");
    let temp = tempfile::TempDir::new().unwrap();
    let case =
        app_services::case_service::create_case(&temp.path().join("cases"), "usb-history", None)
            .unwrap();
    case.with_conn(|conn| {
        DataSourceRepo::new(conn).insert(
            &case.meta.id,
            &domain::DataSource {
                id: domain::DataSourceId("usb-source".into()),
                name: "USB fixture".into(),
                kind: domain::DataSourceKind::E01,
                source_path: image.clone(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )?;
        ArtifactRepo::new(conn).insert_batch(&artifacts, &case.meta.id.0, "usb-source")?;
        let summary = analysis_service::get_registry_structured_summary(conn).unwrap();
        assert_eq!(summary.usb_devices.len(), artifacts.len());
        assert_eq!(
            summary.usb_devices[0].device_name,
            "Samsung Portable SSD T5 SCSI Disk Device"
        );
        eprintln!(
            "USB summary: {}",
            serde_json::to_string_pretty(&summary.usb_devices).unwrap()
        );
        Ok(())
    })
    .unwrap();
    let after = std::fs::metadata(&image).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
}
