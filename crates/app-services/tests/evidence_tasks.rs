use app_services::{
    case_service,
    digest_service::{
        calculate_evidence_digest_with_cancel, DigestExecution, DigestServiceError, DigestTarget,
    },
    evidence_tasks,
};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, evidence_task_repo::EvidenceTaskRepo,
};
use std::sync::atomic::{AtomicBool, Ordering};
use transport::{
    commands::CalculateEvidenceDigestRequest,
    dto::{DigestAlgorithmDto, DigestScopeDto, EvidenceTaskRequestDto, EvidenceTaskResultDto},
};

fn request() -> EvidenceTaskRequestDto {
    EvidenceTaskRequestDto::Digest(CalculateEvidenceDigestRequest {
        scope: DigestScopeDto::ContainerFile,
        algorithm: DigestAlgorithmDto::Sha256,
        data_source_id: Some("source".into()),
        file_id: None,
        partition_index: None,
    })
}
fn setup() -> (
    tempfile::TempDir,
    app_services::active_case::ActiveCase,
    std::path::PathBuf,
) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, b"evidence").unwrap();
    let active = case_service::create_case(&temp.path().join("cases"), "tasks", None).unwrap();
    active
        .with_conn(|conn| {
            DataSourceRepo::new(conn).insert(
                &active.meta.id,
                &DataSource {
                    id: DataSourceId("source".into()),
                    name: "source".into(),
                    kind: DataSourceKind::Raw,
                    source_path: path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
        })
        .unwrap();
    (temp, active, path)
}
#[test]
fn planning_deduplicates_and_persists_a_committed_result_even_after_late_cancel() {
    let (_temp, active, _) = setup();
    active
        .with_conn(|conn| {
            let first =
                evidence_tasks::plan(conn, &active.case_root, &active.meta.id, request()).unwrap();
            let repeated =
                evidence_tasks::plan(conn, &active.case_root, &active.meta.id, request()).unwrap();
            assert!(first.launch);
            assert!(!repeated.launch);
            assert_eq!(first.task.task_id, repeated.task.task_id);
            EvidenceTaskRepo::new(conn)
                .request_cancel(&active.meta.id.0, &first.task.task_id)
                .unwrap();
            persistence_sqlite::repositories::job_repo::JobRepo::new(conn)
                .mark_cancelling(
                    &domain::JobId(first.task.task_id.clone()),
                    "late cancellation",
                )
                .unwrap();
            let result = transport::dto::EvidenceDigestDto {
                scope: DigestScopeDto::ContainerFile,
                algorithm: DigestAlgorithmDto::Sha256,
                value: "a".repeat(64),
                byte_length: 8,
                status: transport::dto::DigestStatusDto::Completed,
            };
            evidence_tasks::finish(
                conn,
                &active.meta.id,
                &first.task.task_id,
                Ok(EvidenceTaskResultDto::Digest(result)),
            )
            .unwrap();
            let completed =
                evidence_tasks::get(conn, &active.meta.id, &first.task.task_id).unwrap();
            assert_eq!(completed.state, "completed");
            assert_eq!((completed.bytes_processed, completed.total_bytes), (8, 8));
            assert!(completed.result.is_some());
            assert!(conn
                .query_row(
                    "SELECT status='completed' AND progress=100 FROM jobs WHERE id=?1",
                    [first.task.task_id],
                    |row| row.get::<_, bool>(0)
                )
                .unwrap());
            assert!(
                !evidence_tasks::plan(conn, &active.case_root, &active.meta.id, request())
                    .unwrap()
                    .launch
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn interrupted_attempt_is_retryable_and_foreign_case_cannot_read_it() {
    let (_temp, active, _) = setup();
    active
        .with_conn(|conn| {
            let first =
                evidence_tasks::plan(conn, &active.case_root, &active.meta.id, request()).unwrap();
            assert!(evidence_tasks::get(
                conn,
                &domain::CaseId("foreign".into()),
                &first.task.task_id
            )
            .is_err());
            app_services::job_service::recover_interrupted_jobs(conn).unwrap();
            let recovered =
                evidence_tasks::get(conn, &active.meta.id, &first.task.task_id).unwrap();
            assert_eq!(recovered.state, "failed");
            assert_eq!(recovered.error_code.as_deref(), Some("interrupted"));
            let retry =
                evidence_tasks::plan(conn, &active.case_root, &active.meta.id, request()).unwrap();
            assert!(retry.launch);
            assert_ne!(retry.task.task_id, first.task.task_id);
            Ok(())
        })
        .unwrap();
}
#[test]
fn actual_source_change_creates_a_distinct_revision_and_attempt() {
    let (_temp, active, path) = setup();
    active
        .with_conn(|conn| {
            let first =
                evidence_tasks::plan(conn, &active.case_root, &active.meta.id, request()).unwrap();
            std::fs::write(path, b"changed evidence and length").unwrap();
            let changed =
                evidence_tasks::plan(conn, &active.case_root, &active.meta.id, request()).unwrap();
            assert!(changed.launch);
            assert_ne!(first.task.source_revision, changed.task.source_revision);
            Ok(())
        })
        .unwrap();
}
#[test]
fn container_cancel_or_mutation_never_persists_a_digest() {
    for mutation in [false, true] {
        let (_temp, active, path) = setup();
        active
            .with_conn(|conn| {
                let cancel = AtomicBool::new(false);
                let result = calculate_evidence_digest_with_cancel(
                    conn,
                    &active.case_root,
                    &active.meta.id,
                    DigestScopeDto::ContainerFile,
                    DigestAlgorithmDto::Sha256,
                    DigestTarget {
                        data_source_id: Some("source"),
                        file_id: None,
                        partition_index: None,
                    },
                    DigestExecution {
                        cancelled: &cancel,
                        progress: &|_, _| {
                            if mutation {
                                std::fs::write(&path, b"changed during read").unwrap();
                            } else {
                                cancel.store(true, Ordering::Release);
                            }
                        },
                    },
                );
                if mutation {
                    assert!(matches!(result, Err(DigestServiceError::EvidenceChanged)));
                } else {
                    assert!(matches!(
                        result,
                        Err(DigestServiceError::Hash(
                            app_services::hash_service::EvidenceHashError::Cancelled
                        ))
                    ));
                }
                assert_eq!(
                    conn.query_row("SELECT COUNT(*) FROM evidence_digests", [], |row| row
                        .get::<_, u64>(0))
                        .unwrap(),
                    0
                );
                Ok(())
            })
            .unwrap();
    }
}
#[test]
fn every_algorithm_has_a_persistent_result_and_an_audit_record() {
    let (_temp, active, _) = setup();
    active
        .with_conn(|conn| {
            for algorithm in [
                DigestAlgorithmDto::Md5,
                DigestAlgorithmDto::Sha1,
                DigestAlgorithmDto::Sha256,
                DigestAlgorithmDto::Sm3,
            ] {
                app_services::digest_service::calculate_evidence_digest(
                    conn,
                    &active.case_root,
                    &active.meta.id,
                    DigestScopeDto::ContainerFile,
                    algorithm,
                    DigestTarget {
                        data_source_id: Some("source"),
                        file_id: None,
                        partition_index: None,
                    },
                )
                .unwrap();
            }
            let rows = app_services::digest_service::results::list(
                conn,
                &active.case_root,
                &active.meta.id,
                transport::dto::DigestResultsRequestDto {
                    data_source_id: "source".into(),
                    file_id: None,
                    partition_index: None,
                },
            )
            .unwrap();
            assert_eq!(rows.len(), 4);
            assert!(rows.iter().all(|row| !row.stale));
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action='evidence.digest'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
                4
            );
            Ok(())
        })
        .unwrap();
}
