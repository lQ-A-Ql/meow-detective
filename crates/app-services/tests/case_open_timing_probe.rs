//! Timing probe for case cold-start: imports a real image once into a
//! persistent workspace, then measures `open_case` across repeated opens.
//! Run with FORENSICS_EMULATION_TEST_E01 or FORENSICS_E01_FIXTURE set:
//!
//! ```text
//! cargo test -p app-services --test case_open_timing_probe -- --include-ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_pipeline::{execute_import_job_with_counts, ImportJobOptions};
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::job_repo::JobRepo;

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_EMULATION_TEST_E01")
        .map(PathBuf::from)
        .or_else(testing::fixtures::local_e01_fixture)
        .expect("set FORENSICS_EMULATION_TEST_E01 or FORENSICS_E01_FIXTURE")
}

#[test]
#[ignore = "requires a real E01 image and takes minutes for the initial import"]
fn case_open_stage_timing_probe() {
    let _subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,app_services=info")),
        )
        .with_writer(std::io::stderr)
        .try_init();
    let temp = tempfile::TempDir::new().unwrap();
    let case_root = temp.path().join("cases/probe");
    {
        std::fs::create_dir_all(temp.path().join("cases")).unwrap();
        let active = app_services::case_service::create_case(
            temp.path().join("cases").as_path(),
            "probe",
            Some("tester"),
        )
        .unwrap();
        let image = sample_path();
        active
            .with_conn(|case_conn| {
                let config = app_services::import_precheck::prepare_import_source_config_from_path(
                    &image.to_string_lossy(),
                    DataSourcePlatform::Windows,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                let job_id = JobRepo::new(case_conn).create(&active.meta.id.0, "probe import")?;
                let cancel = Arc::new(AtomicBool::new(false));
                let outcome = execute_import_job_with_counts(
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
                );
                if let Err(error) = outcome {
                    let detail = case_conn
                        .query_row(
                            "SELECT status, error FROM jobs WHERE id = ?1",
                            [&job_id.0],
                            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                        )
                        .ok();
                    eprintln!("import failed: {} job={:?}", error.message, detail);
                    return Err(persistence_sqlite::DbError::System(error.message));
                }
                Ok(())
            })
            .expect("import the probe image");
        drop(active);
    }

    for attempt in 1..=3 {
        let started = Instant::now();
        let active = app_services::case_service::open_case(&case_root).expect("open probe case");
        eprintln!("open #{attempt}: total={:?}", started.elapsed());
        let migrate_started = Instant::now();
        active
            .with_conn(|conn| {
                app_services::source_db::migrate_ready_source_databases(
                    conn,
                    &active.case_root,
                    &active.meta.id,
                )
            })
            .expect("migrate ready source databases");
        eprintln!(
            "open #{attempt}: migrate_ready_source_databases={:?}",
            migrate_started.elapsed()
        );
        drop(active);
    }
}
