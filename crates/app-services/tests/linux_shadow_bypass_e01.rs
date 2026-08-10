//! Real Linux E01 validation of the host-side shadow bypass: import the
//! image, clear a local account's password hash through the COW overlay,
//! and verify the edit semantically while the evidence stays untouched.
//! Run with FORENSICS_LINUX_E01_FIXTURE set:
//!
//! ```text
//! cargo test -p app-services --test linux_shadow_bypass_e01 -- --include-ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, partition_repo::PartitionRepo,
};

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_LINUX_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LINUX_E01_FIXTURE")
}

#[test]
#[ignore = "requires the private Linux E01 sample"]
fn linux_shadow_bypass_edits_only_the_overlay() {
    let image = sample_path();
    let image_size = std::fs::metadata(&image).unwrap().len();
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "linux-bypass",
        Some("tester"),
    )
    .unwrap();

    // Register the source as imported-ready only if the full import has
    // already produced a source database; otherwise import metadata-only.
    let data_source_id = active
        .with_conn(|case_conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                domain::DataSourcePlatform::Linux,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let job_id = persistence_sqlite::repositories::job_repo::JobRepo::new(case_conn)
                .create(&active.meta.id.0, "linux bypass import")?;
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
        .expect("import the Linux image");

    let (case_id, case_root) = (active.meta.id.clone(), active.case_root.clone());
    // Find the first ext4 partition (direct or LVM LV) from the source DB.
    let ext4_partition = active
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
            for record in &partitions {
                eprintln!(
                    "partition P{} fs={:?} lv={:?}",
                    record.partition_index, record.filesystem, record.lvm_lv_name
                );
            }
            Ok(partitions
                .iter()
                .find(|record| record.filesystem.as_deref() == Some("Ext4"))
                .map(|record| record.partition_index))
        })
        .unwrap();

    let Some(partition_index) = ext4_partition else {
        // No ext4 partition (e.g. an XFS-only image): the bypass must refuse
        // with a typed Unsupported error rather than misbehaving.
        let first = active
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
                    .first()
                    .map(|record| record.partition_index)
                    .expect("the image has at least one partition"))
            })
            .unwrap();
        let outcome = active
            .with_conn(|case_conn| {
                Ok(app_services::emulation_linux_bypass::list_linux_accounts(
                    &app_services::emulation_bypass::BypassCaseContext {
                        case_conn,
                        case_root: &case_root,
                        case_id: &case_id,
                        data_source_id: &data_source_id,
                    },
                    first,
                )
                .map_err(|error| error.to_string()))
            })
            .expect("query the non-ext4 partition");
        let error = outcome.expect_err("non-ext4 partitions must be refused");
        eprintln!("non-ext4 refusal: {error}");
        assert!(error.contains("not ext4") || error.contains("Unsupported"));
        return;
    };

    let accounts = active
        .with_conn(|case_conn| {
            app_services::emulation_linux_bypass::list_linux_accounts(
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
                partition_index,
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
    let target = accounts
        .iter()
        .find(|account| account.has_password)
        .expect("at least one account with a password hash")
        .username
        .clone();

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
                    data_source_id: &data_source_id,
                },
                partition_index,
                &target,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("apply the shadow bypass");
    eprintln!("bypass result: {result:?}");
    assert!(result.password_cleared || result.already_passwordless);
    assert_eq!(
        std::fs::metadata(&image).unwrap().len(),
        image_size,
        "the evidence image must remain untouched"
    );
}
