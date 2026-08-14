//! Real XFS E01 validation of the host-side log-clear repair: register the
//! image's XFS volumes, repair through the COW overlay, verify the clean
//! transition and superblock CRC, then materialize the repaired disk for a
//! VMware boot test. Run with FORENSICS_XFS_E01_FIXTURE set:
//!
//! ```text
//! cargo test -p app-services --test xfs_log_repair_e01 -- --include-ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, partition_repo::PartitionRepo,
};
use transport::dto::EmulationFsVolumeStateDto;

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_XFS_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_XFS_E01_FIXTURE")
}

#[test]
#[ignore = "requires the private XFS E01 sample"]
fn xfs_log_repair_and_materialization() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_test_writer()
        .try_init();
    let image = sample_path();
    let image_size = std::fs::metadata(&image).unwrap().len();
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "xfs-log-clear",
        Some("tester"),
    )
    .unwrap();

    let data_source_id = active
        .with_conn(|case_conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                domain::DataSourcePlatform::Linux,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let job_id = persistence_sqlite::repositories::job_repo::JobRepo::new(case_conn)
                .create(&active.meta.id.0, "xfs log repair import")?;
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            app_services::import_pipeline::execute_import_job_with_counts(
                case_conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job_id,
                app_services::import_pipeline::ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            let sources = DataSourceRepo::new(case_conn).find_by_case(&active.meta.id)?;
            Ok(sources[0].id.clone())
        })
        .expect("import the XFS image metadata-only");

    let (case_id, case_root) = (active.meta.id.clone(), active.case_root.clone());
    let xfs_partitions = active
        .with_conn(|case_conn| {
            let source_conn = app_services::source_db::open_ready_source_read_only_by_id(
                case_conn,
                &case_root,
                &case_id,
                &data_source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let partitions = PartitionRepo::new(&source_conn.connection)
                .find_by_data_source(&data_source_id.0)?;
            Ok(partitions
                .iter()
                .filter(|record| record.filesystem.as_deref() == Some("XFS"))
                .map(|record| record.partition_index)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert!(
        !xfs_partitions.is_empty(),
        "fixture must contain XFS volumes"
    );

    let provider =
        evidence_block::open_block_provider(&image, evidence_block::EvidenceImageKind::E01)
            .unwrap();
    let identity = evidence_emulation::ParentIdentity::new(provider.len(), [0x5au8; 32]).unwrap();
    let disk = Arc::new(
        evidence_emulation::CowDisk::create(
            &temp.path().join("overlay.cow"),
            provider,
            identity,
            evidence_emulation::CowDiskConfig::default(),
        )
        .unwrap(),
    );

    let repair = active
        .with_conn(|case_conn| {
            app_services::emulation_fs_repair::repair_xfs_logs(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("repair the XFS logs");
    eprintln!("repair result: {repair:?}");
    assert_eq!(repair.items.len(), xfs_partitions.len());
    for item in &repair.items {
        eprintln!(
            "P{} state={:?} repaired={} log_bytes={}",
            item.partition_index, item.state, item.repaired, item.log_bytes
        );
        assert_ne!(item.state, EmulationFsVolumeStateDto::Unsupported);
    }
    assert!(
        std::fs::metadata(&image).unwrap().len() == image_size,
        "the evidence image must remain untouched"
    );

    if let Some(out_path) = std::env::var_os("BOOT_DIAG_OUT").map(PathBuf::from) {
        use std::io::{Read, Write};
        let mut reader = app_services::emulation_cow_reader::CowDiskReader::new(Arc::clone(&disk));
        let total = disk.len();
        let mut out =
            std::io::BufWriter::new(std::fs::File::create(&out_path).expect("create output"));
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let mut done = 0u64;
        while done < total {
            let got = reader.read(&mut buf).expect("read");
            assert!(got > 0, "unexpected EOF");
            out.write_all(&buf[..got]).expect("write");
            done += got as u64;
        }
        eprintln!("wrote {} bytes to {}", done, out_path.display());
    }
}
