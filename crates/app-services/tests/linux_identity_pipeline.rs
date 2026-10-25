use app_services::{
    analysis_service, case_service, cluster_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_profiled, ImportJobOptions, ImportProfiler},
};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::sync::{atomic::AtomicBool, Arc};

#[test]
fn raw_ext4_import_catalog_and_artifact_identity_agree_on_primary_file_and_symlink() {
    let inputs = [
        (
            testing::builders::ext4::linux_root_ext4_image_os_release_files(
                b"ID=primary\nPRETTY_NAME='Primary Linux'\nVERSION_ID=1\n",
                b"ID=vendor\nPRETTY_NAME='Vendor Linux'\nVERSION_ID=2\n",
            ),
            "Primary Linux",
            "primary",
        ),
        (
            testing::builders::ext4::linux_root_ext4_image_os_release_symlink(
                b"../usr/lib/os-release",
            ),
            "CentOS Linux 7 (Core)",
            "centos",
        ),
    ];
    for (bytes, expected_name, expected_id) in inputs {
        let workspace = tempfile::tempdir().unwrap();
        let evidence = workspace.path().join("identity.raw");
        std::fs::write(&evidence, &bytes).unwrap();
        let active = case_service::create_case(workspace.path(), "identity", None).unwrap();
        let config = app_services::import_precheck::prepare_import_source_config_from_path(
            evidence.to_str().unwrap(),
            domain::DataSourcePlatform::Linux,
        )
        .unwrap();
        active
            .with_conn(|connection| {
                let job = JobRepo::new(connection).create(&active.meta.id.0, "OS identity")?;
                JobRepo::new(connection).start(&job)?;
                let cancel = Arc::new(AtomicBool::new(false));
                let (message, _) = execute_import_job_profiled(
                    connection,
                    &active.meta.id,
                    &active.case_root,
                    config,
                    &job,
                    ImportJobOptions {
                        projection_policy: Default::default(),
                        event_sink: None,
                        cancel_token: &cancel,
                        max_import_workers: Some(1),
                        max_analysis_workers: Some(1),
                        analysis_mode: ImportAnalysisMode::MetadataOnly,
                        defer_linux_artifacts: true,
                    },
                    &ImportProfiler::default(),
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                JobRepo::new(connection).complete_committed(&job, &message)?;
                let sources = DataSourceRepo::new(connection).find_by_case(&active.meta.id)?;
                assert_eq!(sources.len(), 1);
                let source_id = &sources[0].id;
                let facts = cluster_service::collect_linux_evidence_facts(
                    connection,
                    &active.case_root,
                    &active.meta.id,
                    source_id,
                );
                assert_eq!(
                    facts.operating_system.as_deref(),
                    Some(expected_name),
                    "{:?}",
                    facts.diagnostics
                );
                let run = analysis_service::run_source_analysis_extraction(
                    connection,
                    &active.case_root,
                    &active.meta.id,
                    source_id,
                    &["LinuxSystemConfig"],
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                assert!(run.artifact_count > 0);
                let summary = analysis_service::get_source_linux_summary(
                    connection,
                    &active.case_root,
                    &active.meta.id,
                    source_id,
                    0,
                    200,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                let identity = summary.system_info.unwrap();
                assert_eq!(identity.os_pretty_name.as_deref(), Some(expected_name));
                assert_eq!(identity.os_id.as_deref(), Some(expected_id));
                let replay = analysis_service::run_source_analysis_extraction(
                    connection,
                    &active.case_root,
                    &active.meta.id,
                    source_id,
                    &["LinuxSystemConfig"],
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                assert!(replay.checkpoint_hit_count > 0);
                Ok(())
            })
            .unwrap();
        assert_eq!(std::fs::read(&evidence).unwrap(), bytes);
    }
}
