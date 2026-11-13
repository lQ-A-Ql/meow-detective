use app_services::{
    case_service, digest_service, file_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    import_precheck, source_db,
};
use domain::DataSourcePlatform;
use persistence_source::{detail, repositories::file_repo::FileRepo};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, evidence_digest_repo::EvidenceDigestRepo, job_repo::JobRepo,
};
use std::sync::{atomic::AtomicBool, Arc};
use transport::{
    commands::GetFileRowsRequest,
    dto::{DigestAlgorithmDto, DigestResultsRequestDto, DigestScopeDto},
};

#[test]
fn file_digests_are_persisted_without_changing_the_sealed_catalog() {
    let temporary = tempfile::tempdir().unwrap();
    let evidence = temporary.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    let path = evidence.join("payload.txt");
    let bytes = b"current evidence bytes";
    std::fs::write(&path, bytes).unwrap();
    let active =
        case_service::create_case(&temporary.path().join("cases"), "file-digest", None).unwrap();
    active
        .with_conn(|connection| {
            let job = JobRepo::new(connection).create(&active.meta.id.0, "digest fixture")?;
            assert!(JobRepo::new(connection).start(&job)?);
            let config = import_precheck::prepare_import_source_config_from_path(
                evidence.to_str().unwrap(),
                DataSourcePlatform::Linux,
            )
            .unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            execute_import_job_with_counts(
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
            )
            .unwrap();
            let source = DataSourceRepo::new(connection).find_by_case(&active.meta.id)?;
            assert_eq!(source.len(), 1);
            let source_id = &source[0].id;
            let source_connection = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                source_id,
            )?;
            let file = FileRepo::new(&source_connection)
                .find_by_data_source(source_id)?
                .into_iter()
                .find(|file| file.name == "payload.txt")
                .unwrap();
            let store = detail::require_source_store(&source_connection)?;
            let before = persistence_source::fjall::verify_source_generation_with_control(
                &store,
                &source_id.0,
                &AtomicBool::new(false),
                |_, _| {},
            )
            .unwrap();
            let global = source_db::GlobalFileId::new(source_id.clone(), file.id.clone())
                .encode()
                .0;
            let digest = digest_service::calculate_evidence_digest(
                connection,
                &active.case_root,
                &active.meta.id,
                DigestScopeDto::File,
                DigestAlgorithmDto::Sha256,
                digest_service::DigestTarget {
                    data_source_id: Some(&source_id.0),
                    file_id: Some(&global),
                    partition_index: None,
                },
            )
            .unwrap();
            assert_eq!(digest.byte_length, bytes.len() as u64);
            assert_eq!(
                digest.value,
                infrastructure::hashing::digest_bytes(
                    bytes,
                    infrastructure::hashing::HashAlgorithm::Sha256,
                )
            );
            let saved = EvidenceDigestRepo::new(connection).file_sha256_values(
                &active.meta.id.0,
                &source_id.0,
                &[&global],
            )?;
            assert_eq!(saved.len(), 1);
            assert_eq!(saved[0].value, digest.value);
            let metadata = file_service::get_file_metadata_for_case(
                connection,
                &active.case_root,
                &active.meta.id,
                &global,
            )
            .unwrap();
            assert_eq!(metadata.hash_sha256.as_deref(), Some(digest.value.as_str()));
            let parent =
                source_db::GlobalFileId::new(source_id.clone(), file.parent_id.clone().unwrap())
                    .encode()
                    .0;
            let page = file_service::get_file_rows_for_case(
                connection,
                &active.case_root,
                &active.meta.id,
                &GetFileRowsRequest {
                    parent_id: Some(parent),
                    show_hidden: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let row = page.rows.iter().find(|row| row.id == global).unwrap();
            assert_eq!(row.hash_sha256.as_deref(), Some(digest.value.as_str()));
            let after = persistence_source::fjall::verify_source_generation_with_control(
                &store,
                &source_id.0,
                &AtomicBool::new(false),
                |_, _| {},
            )
            .unwrap();
            assert_eq!(before, after);
            assert!(FileRepo::new(&source_connection)
                .find_by_id(&file.id)?
                .unwrap()
                .hash_sha256
                .is_none());
            let cancelled = AtomicBool::new(true);
            let result = digest_service::calculate_evidence_digest_with_cancel(
                connection,
                &active.case_root,
                &active.meta.id,
                DigestScopeDto::File,
                DigestAlgorithmDto::Md5,
                digest_service::DigestTarget {
                    data_source_id: Some(&source_id.0),
                    file_id: Some(&global),
                    partition_index: None,
                },
                digest_service::DigestExecution {
                    cancelled: &cancelled,
                    progress: &|_, _| {},
                },
            );
            assert!(matches!(
                result,
                Err(digest_service::DigestServiceError::Hash(
                    app_services::hash_service::EvidenceHashError::Cancelled
                ))
            ));
            assert_eq!(
                EvidenceDigestRepo::new(connection)
                    .list(&active.meta.id.0, &source_id.0, Some(&global), None)?
                    .len(),
                1
            );
            std::fs::write(&path, b"changed evidence with a different size").unwrap();
            let results = digest_service::results::list(
                connection,
                &active.case_root,
                &active.meta.id,
                DigestResultsRequestDto {
                    data_source_id: source_id.0.clone(),
                    file_id: Some(global.clone()),
                    partition_index: None,
                },
            )
            .unwrap();
            assert_eq!(results.len(), 1);
            assert!(results[0].stale);
            assert!(file_service::get_file_metadata_for_case(
                connection,
                &active.case_root,
                &active.meta.id,
                &global,
            )
            .unwrap()
            .hash_sha256
            .is_none());
            Ok(())
        })
        .unwrap();
}
