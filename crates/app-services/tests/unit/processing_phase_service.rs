use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim,
        ProcessingPhaseCompletion, ProcessingPhaseTransition,
    },
};
use std::path::PathBuf;

use super::get_data_source_processing_summary;

fn insert_derived_source(connection: &rusqlite::Connection) -> DataSourceId {
    persistence_sqlite::runner::run_all(connection).expect("run case migrations");
    let case_id = CaseId("processing-phase-case".to_string());
    connection
        .execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES (?1, 'Processing Test', datetime('now'), datetime('now'))",
            [&case_id.0],
        )
        .expect("insert case");
    let source = DataSource {
        id: DataSourceId("processing-phase-source".to_string()),
        name: "Derived VM".to_string(),
        kind: DataSourceKind::CephRbd,
        source_path: PathBuf::from("ceph-rbd://cluster/image"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    DataSourceRepo::new(connection)
        .insert(&case_id, &source)
        .expect("insert derived source");
    source.id
}

fn finish_ready(
    connection: &rusqlite::Connection,
    source_id: &DataSourceId,
    phase: ProcessingPhase,
) {
    let repository = DataSourceProcessingPhaseRepo::new(connection);
    let claim = repository
        .claim(source_id, phase, 1, &"a".repeat(64), "test-owner")
        .expect("claim phase");
    let ProcessingPhaseClaim::Acquired(record) = claim else {
        panic!("phase should be acquired");
    };
    repository
        .finish(
            source_id,
            phase,
            ProcessingPhaseCompletion::new(
                1,
                &"a".repeat(64),
                record.owner_id.as_deref().expect("owner"),
                record.attempt_id.as_deref().expect("attempt"),
                ProcessingPhaseTransition::ready(r#"{"rows":1}"#),
            ),
        )
        .expect("finish phase");
}

#[test]
fn summary_is_absent_when_no_processing_ledger_exists() {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    let source_id = insert_derived_source(&connection);

    let summary =
        get_data_source_processing_summary(&connection, &source_id).expect("query summary");

    assert!(summary.is_none());
}

#[test]
fn summary_aggregates_backend_phase_state_without_frontend_inference() {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    let source_id = insert_derived_source(&connection);
    finish_ready(&connection, &source_id, ProcessingPhase::Catalog);
    DataSourceProcessingPhaseRepo::new(&connection)
        .upsert(&source_id, ProcessingPhase::Graph, 1, &"b".repeat(64))
        .expect("insert pending phase");

    let summary = get_data_source_processing_summary(&connection, &source_id)
        .expect("query summary")
        .expect("processing summary");

    assert_eq!(summary.state, "pending");
    assert_eq!(summary.total_count, 2);
    assert_eq!(summary.ready_count, 1);
    assert_eq!(summary.pending_count, 1);
    assert_eq!(summary.phases[0].phase, "catalog");
    assert_eq!(summary.phases[0].state, "ready");
    assert_eq!(summary.phases[1].phase, "graph");
    assert_eq!(summary.phases[1].state, "pending");
}
