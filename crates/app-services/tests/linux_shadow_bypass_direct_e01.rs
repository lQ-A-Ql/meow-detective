//! Direct-registration Linux E01 validation of the shadow bypass: probe the
//! image for partitions (seconds) and register them into the source database
//! directly, skipping the full enumeration import whose timeline
//! materialization on multi-million-entry images exceeds the E2E budget.
//! Run with FORENSICS_LINUX_E01_FIXTURE set:
//!
//! ```text
//! cargo test -p app-services --test linux_shadow_bypass_direct_e01 -- --include-ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use domain::{DataSourceKind, DataSourcePlatform};
use image_e01::E01Reader;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_LINUX_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LINUX_E01_FIXTURE")
}

#[test]
#[ignore = "requires the private Linux E01 sample"]
fn linux_shadow_bypass_with_direct_partition_registration() {
    let image = sample_path();
    let image_size = std::fs::metadata(&image).unwrap().len();

    let mut probe_reader = E01Reader::open(&image).unwrap();
    let probe = app_services::datasource_service::detect_image_filesystem(&mut probe_reader)
        .expect("probe the image filesystems");
    for partition in &probe.partitions {
        eprintln!(
            "probe P{} fs={:?} offset={} length={}",
            partition.index, partition.filesystem, partition.offset, partition.length
        );
    }
    let target = probe
        .partitions
        .iter()
        .find(|partition| {
            partition.filesystem
                == Some(app_services::datasource_service::ImageFilesystemKind::Ext4)
        })
        .expect("the image must expose an ext4 partition")
        .clone();

    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "linux-direct",
        Some("tester"),
    )
    .unwrap();
    let ds = active
        .with_conn(|case_conn| {
            app_services::datasource_service::attach_data_source(
                case_conn,
                &active.meta.id,
                "linux-direct",
                &image,
                DataSourceKind::E01,
                DataSourcePlatform::Linux,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("attach the source");
    let source_db = app_services::source_db::source_db_path(&active.case_root, &ds.id);
    std::fs::create_dir_all(source_db.parent().unwrap()).unwrap();
    {
        let source_conn = persistence_sqlite::connection::open_or_create_source(&source_db)
            .expect("create the source database");
        app_services::file_service::store_data_source_partitions(
            &source_conn,
            &ds.id,
            &probe.partitions,
        )
        .expect("store the probed partitions");
    }
    active
        .with_conn(|case_conn| {
            DataSourceRepo::new(case_conn).update_import_state(&ds.id, "ready", None)
        })
        .expect("mark the source ready");

    let (case_id, case_root) = (active.meta.id.clone(), active.case_root.clone());
    let accounts = active
        .with_conn(|case_conn| {
            app_services::emulation_linux_bypass::list_linux_accounts(
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &ds.id,
                },
                target.index as u32,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("list shadow accounts");
    assert!(!accounts.is_empty(), "shadow accounts must be listed");
    for account in &accounts {
        eprintln!(
            "account {} has_password={} locked={}",
            account.username, account.has_password, account.locked
        );
    }
    let Some(target_user) = accounts
        .iter()
        .find(|account| account.has_password)
        .map(|account| account.username.clone())
    else {
        eprintln!("no password-protected account; nothing to bypass");
        return;
    };

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
    let result = active
        .with_conn(|case_conn| {
            app_services::emulation_linux_bypass::apply_linux_bypass(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &ds.id,
                },
                target.index as u32,
                &target_user,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("apply the shadow bypass");
    eprintln!("bypass result: {result:?}");
    assert!(result.password_cleared || result.already_passwordless);
    assert_eq!(std::fs::metadata(&image).unwrap().len(), image_size);
}
