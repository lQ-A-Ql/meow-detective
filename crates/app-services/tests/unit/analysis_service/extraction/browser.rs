use super::sqlite::with_temp_sqlite;
use super::ExtractionOutcome;

fn temp_files(prefix: &str) -> Vec<std::path::PathBuf> {
    let expected_prefix = format!("forensics-{prefix}-");
    std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&expected_prefix) && name.ends_with(".sqlite"))
        })
        .collect()
}

#[test]
fn temporary_sqlite_is_removed_after_successful_parse() {
    let prefix = format!("browser-cleanup-success-{}", uuid::Uuid::new_v4());
    let result = with_temp_sqlite(b"not-a-database", &prefix, |_| {
        Ok(ExtractionOutcome::default())
    });

    assert!(result.is_ok());
    assert!(temp_files(&prefix).is_empty());
}

#[test]
fn temporary_sqlite_is_removed_after_parse_failure() {
    let prefix = format!("browser-cleanup-failure-{}", uuid::Uuid::new_v4());
    let result = with_temp_sqlite(b"not-a-database", &prefix, |_| {
        Err(crate::analysis_service::error::AnalysisServiceError::Other(
            "expected parse failure".to_string(),
        ))
    });

    assert!(result.is_err());
    assert!(temp_files(&prefix).is_empty());
}
