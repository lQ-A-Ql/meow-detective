use super::*;

use chrono::Utc;
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use search::SearchFileDocument;
use tempfile::TempDir;
use transport::commands::{FileSortDirectionDto, SearchEntryTypeDto, SearchSortKeyDto};

fn document(id: &str, name: &str, path: &str, entry_type: &str) -> SearchFileDocument {
    SearchFileDocument {
        file_id: id.to_string(),
        path: path.to_string(),
        name: name.to_string(),
        extension: name
            .rsplit_once('.')
            .map(|(_, ext)| ext)
            .unwrap_or_default()
            .to_string(),
        entry_type: entry_type.to_string(),
        size: Some(0),
        modified_at: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
    }
}

fn request(query: &str, offset: u64, limit: u32) -> SearchFilesRequest {
    SearchFilesRequest {
        query: query.to_string(),
        match_path: false,
        entry_type: SearchEntryTypeDto::Any,
        extensions: Vec::new(),
        data_source_ids: Vec::new(),
        sort_key: SearchSortKeyDto::Name,
        sort_direction: FileSortDirectionDto::Asc,
        offset,
        limit,
        cursor: None,
    }
}

struct SearchRequestFixture {
    case_root: std::path::PathBuf,
    case_id: domain::CaseId,
    case_conn: rusqlite::Connection,
}

fn setup_indexed_case(tmp: &TempDir) -> SearchRequestFixture {
    let active =
        crate::case_service::create_case(&tmp.path().join("cases"), "search-request", None)
            .expect("create case");
    let case_root = active.case_root.clone();
    let case_id = active.meta.id.clone();
    let case_conn =
        crate::connection::open_case_db(&case_root.join("app.db")).expect("open case database");

    let source_id = DataSourceId("ds-1".to_string());
    let source = DataSource {
        id: source_id.clone(),
        name: "source-ds-1".to_string(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: tmp.path().join("ds-1"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db("ds-1", Some("linux"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(&case_id, &source, &storage)
        .expect("register data source");

    let source_conn =
        crate::source_db::open_source_db(&case_root, &source_id).expect("open source");
    let documents = vec![
        document("one", "report.txt", "/Downloads/report.txt", "file"),
        document("two", "report-data", "/Downloads/report-data", "directory"),
    ];
    for entry in &documents {
        source_conn
            .execute(
                "INSERT INTO file_entries
                 (id, data_source_id, path, name, entry_type, size, ext, modified_at,
                  deleted, hidden, system, encrypted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    entry.file_id,
                    "ds-1",
                    entry.path,
                    entry.name,
                    entry.entry_type,
                    entry.size,
                    (!entry.extension.is_empty()).then_some(entry.extension.clone()),
                    Option::<String>::None,
                    0,
                    0,
                    0,
                    0,
                ],
            )
            .unwrap();
    }
    let index_dir = case_root.join("sources").join("ds-1").join("index");
    let index = search::SearchIndex::create(&index_dir).unwrap();
    let mut writer = index.metadata_writer().unwrap();
    writer.add_documents(&documents).unwrap();
    writer.commit().unwrap();

    SearchRequestFixture {
        case_root,
        case_id,
        case_conn,
    }
}

fn recorded_search_params(case_root: &std::path::Path) -> Vec<String> {
    let conn = crate::connection::open_case_db(&case_root.join("app.db")).expect("open case db");
    let mut statement = conn
        .prepare("SELECT params_json FROM investigation_steps WHERE step_kind = 'search'")
        .expect("prepare investigation step query");
    statement
        .query_map([], |row| row.get(0))
        .expect("query investigation steps")
        .collect::<Result<Vec<String>, _>>()
        .expect("read investigation step params")
}

#[test]
fn search_request_reports_performance_and_records_step_for_first_page() {
    let tmp = TempDir::new().unwrap();
    let fixture = setup_indexed_case(&tmp);
    let mut reported = 0u32;

    let page = search_files_request_for_case(
        &fixture.case_conn,
        &fixture.case_root,
        &fixture.case_id,
        &request("report", 0, 20),
        |report| {
            reported += 1;
            assert!(report
                .metrics
                .iter()
                .any(|metric| metric.key == "search.query.rows"));
        },
    )
    .expect("first-page search succeeds");

    assert_eq!(page.total, 2);
    assert_eq!(reported, 1);

    let params = recorded_search_params(&fixture.case_root);
    assert_eq!(params.len(), 1);
    let params: serde_json::Value = serde_json::from_str(&params[0]).expect("parse step params");
    assert_eq!(params["query"], "report");
    assert_eq!(params["offset"], 0);
    assert_eq!(params["limit"], 20);
    assert_eq!(params["cursorContinuation"], false);
    assert_eq!(params["totalHits"], 2);
}

#[test]
fn search_request_uses_offset_path_for_later_pages() {
    let tmp = TempDir::new().unwrap();
    let fixture = setup_indexed_case(&tmp);
    let mut reported = 0u32;

    let page = search_files_request_for_case(
        &fixture.case_conn,
        &fixture.case_root,
        &fixture.case_id,
        &request("report", 1, 20),
        |_report| reported += 1,
    )
    .expect("offset search succeeds");

    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 1);
    assert_eq!(reported, 1);

    let params = recorded_search_params(&fixture.case_root);
    assert_eq!(params.len(), 1);
    let params: serde_json::Value = serde_json::from_str(&params[0]).expect("parse step params");
    assert_eq!(params["offset"], 1);
    assert_eq!(params["cursorContinuation"], false);
    assert_eq!(params["totalHits"], 2);
}
