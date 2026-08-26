//! Real Linux E01 validation of the emulation preflight: import the image,
//! run the catalog+filesystem preflight, and verify the detected installs,
//! distro identity and the derived VMware guest profile. Run with
//! FORENSICS_LINUX_E01_FIXTURE set:
//!
//! ```text
//! cargo test -p app-services --test linux_emulation_preflight_e01 -- --include-ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_pipeline::{execute_import_job_with_counts, ImportJobOptions};
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::repositories::job_repo::JobRepo;
use transport::dto::{EmulationBootRouteDto, EmulationInstallPlatformDto};

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_LINUX_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LINUX_E01_FIXTURE")
}

#[test]
#[ignore = "requires the private Linux E01 sample"]
fn linux_preflight_detects_installs_and_derives_the_guest_profile() {
    let image = sample_path();
    let image_size = std::fs::metadata(&image).unwrap().len();
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "linux-preflight",
        Some("tester"),
    )
    .unwrap();
    let data_source_id = active
        .with_conn(|case_conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                DataSourcePlatform::Linux,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let job_id = JobRepo::new(case_conn).create(&active.meta.id.0, "preflight import")?;
            let cancel = Arc::new(AtomicBool::new(false));
            execute_import_job_with_counts(
                case_conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            let sources = DataSourceRepo::new(case_conn).find_by_case(&active.meta.id)?;
            assert_eq!(sources.len(), 1, "exactly one source must be registered");
            Ok(sources[0].id.clone())
        })
        .expect("import the Linux image");

    let preflight = active
        .with_conn(|case_conn| {
            app_services::mount_service::emulation_preflight(
                case_conn,
                &active.case_root,
                &active.meta.id,
                &data_source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("preflight over the import catalog");

    assert!(
        !preflight.installs.is_empty(),
        "a Linux system image must expose at least one installation"
    );
    for install in &preflight.installs {
        eprintln!(
            "install P{}: platform={:?} distro={:?} kernel={:?} fstab={:?} risks={:?}",
            install.partition_index,
            install.platform,
            install.os_release_pretty_name,
            install.kernel_present,
            install.fstab_present,
            install.boot_risk_notes
        );
        assert_eq!(install.platform, EmulationInstallPlatformDto::Linux);
        assert!(
            install.os_release_pretty_name.is_some(),
            "the filesystem probe must read the distro pretty name"
        );
    }
    assert_eq!(
        preflight.recommended_boot_route,
        EmulationBootRouteDto::DirectSystem,
        "Linux installs always recommend the direct boot route"
    );

    let profile = active
        .with_conn(|case_conn| {
            app_services::mount_service::linux_guest_profile(
                case_conn,
                &active.case_root,
                &active.meta.id,
                &data_source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("derive the guest profile");
    eprintln!(
        "guest OS profile: {} adapter={:?} reason={}",
        profile.guest_os, profile.disk_adapter, profile.disk_adapter_reason
    );
    assert!(
        evidence_emulation::GUEST_OS_WHITELIST.contains(&profile.guest_os.as_str()),
        "the derived guest OS must be renderable: {}",
        profile.guest_os
    );

    assert_eq!(std::fs::metadata(&image).unwrap().len(), image_size);
}
