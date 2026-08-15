use app_services::analysis_service::{
    extract_system_info_for_case, get_evidence_classification_summary,
};
use domain::{DataSourceId, DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use transport::dto::AnalysisParseStatusDto;

const SOURCE_ID: &str = "source-provenance";

fn source_connection() -> rusqlite::Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
}

fn insert_source_file(connection: &rusqlite::Connection, id: &str, path: &str) {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    FileRepo::new(connection)
        .insert_batch(&[FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId(SOURCE_ID.to_string()),
            path: path.to_string(),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(4),
            ext: path.rsplit_once('.').map(|(_, ext)| ext.to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .expect("insert source file");
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
    let connection = source_connection();

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
        .any(|warning| warning.contains("Windows/System32/config/SYSTEM")));
}
