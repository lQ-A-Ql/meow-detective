use super::{
    config::RunConfig,
    metrics::{measure, ScenarioCounters},
    native_fixture::NativeFixture,
    native_records::seed_artifacts,
};

pub(super) fn run(config: &RunConfig) -> serde_json::Value {
    let fixture = NativeFixture::new(1, config.scale.correlation_files);
    let files = fixture.files(0);
    assert_eq!(files.len(), config.scale.correlation_files);
    seed_artifacts(&fixture, 250, Some(&files));
    fixture
        .active
        .with_conn(|connection| {
            Ok(measure(
                config,
                "correlation_paths",
                serde_json::json!({"files": files.len(), "artifacts": 250, "sources": 1}),
                || {
                    let snapshot =
                        app_services::correlation::get_correlation_snapshot_read_only_for_case(
                            connection,
                            &fixture.active.case_root,
                            &fixture.active.meta.id,
                        )
                        .unwrap();
                    assert!(!snapshot.leads.is_empty());
                    ScenarioCounters {
                        returned_rows: Some(snapshot.leads.len() as u64),
                        ..Default::default()
                    }
                },
            ))
        })
        .unwrap()
}
