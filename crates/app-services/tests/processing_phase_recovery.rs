use app_services::processing_phase_service::recover_interrupted_processing_phases;
use domain::DataSourceId;
use persistence_sqlite::{
    repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim, ProcessingPhaseState,
    },
    runner,
};

const FINGERPRINT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn case_recovery_fails_running_processing_phases() {
    let connection = persistence_sqlite::open_in_memory().expect("open case database");
    runner::run_all(&connection).expect("run migrations");
    connection
        .execute_batch(
            "INSERT INTO cases (id, name) VALUES ('case-1', 'Recovery')
             ; INSERT INTO data_sources (id, case_id, name, kind, source_path)
               VALUES ('rbd-source', 'case-1', 'VM disk', 'ceph_rbd', 'ceph-rbd://cluster/vm');",
        )
        .expect("insert case and derived source");

    let source_id = DataSourceId("rbd-source".to_string());
    let repository = DataSourceProcessingPhaseRepo::new(&connection);
    let claim = repository
        .claim(
            &source_id,
            ProcessingPhase::Artifacts,
            1,
            FINGERPRINT,
            "previous-process",
        )
        .expect("claim artifacts phase");
    assert!(matches!(claim, ProcessingPhaseClaim::Acquired(_)));

    assert_eq!(
        recover_interrupted_processing_phases(&connection).expect("recover phases"),
        1
    );
    let recovered = repository
        .find(&source_id, ProcessingPhase::Artifacts)
        .expect("query phase")
        .expect("phase exists");
    assert_eq!(recovered.state, ProcessingPhaseState::Failed);
    assert_eq!(
        recovered.last_error.as_deref(),
        Some("Interrupted: application exited unexpectedly")
    );
}
