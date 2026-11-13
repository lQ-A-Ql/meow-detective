use super::{
    config::RunConfig,
    metrics::{measure, ScenarioCounters},
    native_fixture::NativeFixture,
};

pub(super) fn run(config: &RunConfig) -> serde_json::Value {
    let fixture = NativeFixture::new(1, config.scale.header_files);
    let files = fixture.files(0);
    assert_eq!(files.len(), config.scale.header_files);
    let connection = fixture.source_connection(0);
    measure(
        config,
        "file_header_reads",
        serde_json::json!({"files": files.len(), "sources": 1, "headerBytes": 64}),
        || {
            let cache =
                app_services::file_service::FileHeaderReadCache::new(&fixture.active.meta.id.0);
            for file in &files {
                let bytes = cache
                    .read_file_header_by_id(&connection, &file.id, 64)
                    .unwrap();
                assert!(bytes.starts_with(b"benchmark content "));
            }
            ScenarioCounters {
                files_scanned: Some(files.len() as u64),
                ..Default::default()
            }
        },
    )
}
