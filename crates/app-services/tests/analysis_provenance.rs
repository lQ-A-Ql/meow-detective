use app_services::analysis_service::{
    extract_system_info_for_case, get_evidence_classification_summary,
};
use domain::DataSourcePlatform;
use transport::dto::AnalysisParseStatusDto;

#[path = "support/native_analysis_fixture.rs"]
mod native_analysis_fixture;

use native_analysis_fixture::{source_file, NativeAnalysisFixture};

const SOURCE_ID: &str = "source-provenance";

fn source_connection() -> NativeAnalysisFixture {
    NativeAnalysisFixture::new(SOURCE_ID, "case-provenance")
}

fn unknown_source_connection() -> rusqlite::Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open unknown source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
}

fn insert_source_file(connection: &NativeAnalysisFixture, id: &str, path: &str) {
    connection.insert_file(source_file(SOURCE_ID, id, path, 4, false));
}

#[test]
fn evidence_summary_preserves_candidate_data_source_id_in_provenance() {
    let connection = source_connection();
    insert_source_file(&connection, "system-hive", "Windows/System32/config/SYSTEM");

    let summary = get_evidence_classification_summary(&connection, DataSourcePlatform::Windows)
        .expect("build evidence summary");
    let category = summary
        .categories
        .iter()
        .find(|category| category.category == "SystemInformation")
        .expect("system information category");

    assert_eq!(category.provenance.len(), 1);
    assert_eq!(category.provenance[0].data_source_id, SOURCE_ID);
    assert_eq!(
        category.provenance[0].artifact_path,
        "Windows/System32/config/SYSTEM"
    );
}

#[test]
fn system_info_provenance_never_discards_an_attributable_source_id() {
    let connection = source_connection();
    insert_source_file(&connection, "system-hive", "Windows/System32/config/SYSTEM");

    let info = extract_system_info_for_case(
        &connection,
        |_file_id, _max_bytes| -> Result<Vec<u8>, String> { Ok(b"BAD!".to_vec()) },
    );

    assert!(info.provenance.iter().any(|item| item.artifact_path
        == "Windows/System32/config/SYSTEM"
        && item.status == AnalysisParseStatusDto::NotParsed));
    assert!(info
        .provenance
        .iter()
        .all(|item| item.data_source_id == SOURCE_ID));
}

#[test]
fn system_info_omits_unavailable_provenance_when_source_is_unknown() {
    let connection = unknown_source_connection();

    let info = extract_system_info_for_case(
        &connection,
        |_file_id, _max_bytes| -> Result<Vec<u8>, String> {
            panic!("empty source must not read evidence")
        },
    );

    assert_eq!(info.status, AnalysisParseStatusDto::NotParsed);
    assert!(info.provenance.is_empty());
    assert!(info
        .warnings
        .iter()
        .all(|warning| !warning.contains("Windows/System32/config/SYSTEM")));
}
