//! Verifies that Linux artifact extraction is automatic after source import.

use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job, ImportJobOptions},
    import_precheck::prepare_import_source_config_from_path,
    source_db,
};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, job_repo::JobRepo};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
fn linux_logical_import_auto_extracts_identity_and_service_artifacts() {
    let tmp = tempfile::TempDir::new().unwrap();
    let evidence = tmp.path().join("linux-root");
    std::fs::create_dir_all(evidence.join("etc/systemd/system")).unwrap();
    std::fs::create_dir_all(evidence.join("var/lib/docker/containers/abc")).unwrap();
    std::fs::write(evidence.join("etc/hostname"), "auto-linux\n").unwrap();
    std::fs::write(
        evidence.join("etc/os-release"),
        "PRETTY_NAME=\"Test Linux\"\nVERSION_ID=\"9\"\n",
    )
    .unwrap();
    std::fs::write(evidence.join("etc/hosts"), "192.0.2.10 auto-linux\n").unwrap();
    std::fs::write(
        evidence.join("etc/systemd/system/forensic.service"),
        "[Service]\nExecStart=/usr/bin/forensic\n",
    )
    .unwrap();
    std::fs::write(
        evidence.join("var/lib/docker/containers/abc/config.v2.json"),
        r#"{"Name":"/forensic-worker","Config":{"Image":"registry.example/worker:v1"}}"#,
    )
    .unwrap();

    let active =
        case_service::create_case(tmp.path(), "linux-auto-artifacts", Some("test")).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    active
        .with_conn(|conn| {
            let job_id =
                JobRepo::new(conn).create(&active.meta.id.0, "Linux automatic artifact import")?;
            let config = prepare_import_source_config_from_path(
                &evidence.to_string_lossy(),
                domain::DataSourcePlatform::Linux,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let message = execute_import_job(
                conn,
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
            assert!(message.contains("Linux artifacts auto-analyzed"));
            let source_id = conn.query_row(
                "SELECT id FROM data_sources WHERE case_id = ?1",
                [&active.meta.id.0],
                |row| row.get::<_, String>(0),
            )?;
            let source_conn = persistence_sqlite::open_existing_source(
                &source_db::source_db_path(&active.case_root, &domain::DataSourceId(source_id)),
            )?;
            let artifacts = ArtifactRepo::new(&source_conn).count()?;
            assert!(
                artifacts >= 3,
                "automatic Linux extraction should persist identity/config artifacts"
            );
            Ok(())
        })
        .unwrap();
}
