use super::{
    config::RunConfig,
    metrics::{measure, ScenarioCounters},
    native_fixture::NativeFixture,
    native_records::{seed_artifacts, ARTIFACT_TYPE},
};

pub(super) fn run(config: &RunConfig) -> serde_json::Value {
    let fixture = NativeFixture::new(1, 1);
    seed_artifacts(&fixture, config.scale.artifact_records, None);
    fixture.active.with_conn(|connection| {
        Ok(measure(config, "artifact_native_facets",
            serde_json::json!({"records": config.scale.artifact_records, "sources": 1, "types": 1, "requests": 2}),
            || {
                let families = app_services::artifact_service::get_artifact_families_for_case(
                    connection, &fixture.active.case_root, &fixture.active.meta.id,
                ).unwrap();
                let counts = app_services::artifact_service::get_artifact_family_counts_for_case(
                    connection, &fixture.active.case_root, &fixture.active.meta.id,
                ).unwrap();
                assert!(families.iter().any(|family| family == ARTIFACT_TYPE));
                let observed = counts.iter().find(|row| row.family == ARTIFACT_TYPE).unwrap();
                assert_eq!(observed.count, config.scale.artifact_records as u64);
                ScenarioCounters { returned_rows: Some(counts.len() as u64), ..Default::default() }
            },
        ))
    }).unwrap()
}
