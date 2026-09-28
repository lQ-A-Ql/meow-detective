//! Real Linux E01 validation for the emulation network COW rewrite.
//! Run with `FORENSICS_LINUX_E01_FIXTURE` and `--include-ignored`.

use std::path::PathBuf;
use std::sync::Arc;

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_static_network_profile_is_cleared_in_cow_only() {
    let image = PathBuf::from(std::env::var_os("FORENSICS_LINUX_E01_FIXTURE").expect("fixture"));
    let image_size = std::fs::metadata(&image).expect("image metadata").len();
    let mut reader = image_e01::E01Reader::open(&image).expect("open E01");
    let mut probe = app_services::datasource_service::detect_image_filesystem(&mut reader)
        .expect("probe image");
    app_services::datasource_service::expand_lvm_pool_candidates(
        &mut probe,
        &image,
        &domain::DataSourceKind::E01,
    );
    let temp = tempfile::TempDir::new().expect("temp root");
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "network",
        Some("tester"),
    )
    .expect("case");
    let source = active
        .with_conn(|conn| {
            app_services::datasource_service::attach_data_source(
                conn,
                &active.meta.id,
                "network",
                &image,
                domain::DataSourceKind::E01,
                domain::DataSourcePlatform::Linux,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("attach source");
    let db_path = app_services::source_db::source_db_path(&active.case_root, &source.id);
    let db = persistence_sqlite::open_or_create_source(&db_path).expect("source db");
    app_services::file_service::store_data_source_partitions(&db, &source.id, &probe.partitions)
        .expect("partitions");
    active
        .with_conn(|conn| {
            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
                .update_import_state(&source.id, "ready", None)
        })
        .expect("ready");
    let provider =
        evidence_block::open_block_provider(&image, evidence_block::EvidenceImageKind::E01)
            .expect("provider");
    let logical_size = provider.len();
    let disk = Arc::new(
        evidence_emulation::CowDisk::create(
            &temp.path().join("network.cow"),
            provider,
            evidence_emulation::ParentIdentity::new(logical_size, [0x41; 32]).expect("identity"),
            evidence_emulation::CowDiskConfig::default(),
        )
        .expect("cow"),
    );
    let changed = active
        .with_conn(|conn| {
            let source_conn = app_services::source_db::open_ready_source_read_only_by_id(
                conn,
                &active.case_root,
                &active.meta.id,
                &source.id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let rows = persistence_sqlite::repositories::partition_repo::PartitionRepo::new(
                &source_conn.connection,
            )
            .find_by_data_source(&source.id.0)?;
            for row in rows {
                eprintln!(
                    "network test partition P{} fs={:?} lv={:?}",
                    row.partition_index, row.filesystem, row.lvm_lv_name
                );
            }
            app_services::emulation_linux_bypass::prepare_network_for_emulation(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn: conn,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                    data_source_id: &source.id,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("network rewrite");
    eprintln!("network profile rewritten: {changed}");
    assert!(
        changed,
        "real sample must contain a static Linux network profile"
    );
    assert_eq!(
        std::fs::metadata(&image).expect("image metadata").len(),
        image_size
    );
}
