use super::*;

use chrono::Utc;
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use search::SearchFileDocument;
use tempfile::TempDir;
use transport::commands::{
    FileSortDirectionDto, SearchEntryTypeDto, SearchFilesRequest, SearchSortKeyDto,
};

fn setup_case(tmp: &TempDir, source_ids: &[&str]) -> rusqlite::Connection {
    let connection = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    let case = domain::CaseMeta {
        id: CaseId("case-1".to_string()),
        name: "case".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&connection)
        .create(&case)
        .unwrap();
    for source_id in source_ids {
        let id = DataSourceId((*source_id).to_string());
        let source = DataSource {
            id: id.clone(),
            name: format!("source-{source_id}"),
            kind: DataSourceKind::LogicalDirectory,
            source_path: tmp.path().join(source_id),
            imported_at: Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
        storage.import_state = "ready".to_string();
        DataSourceRepo::new(&connection)
            .insert_with_storage(&CaseId("case-1".to_string()), &source, &storage)
            .unwrap();
        crate::source_db::open_source_db(tmp.path(), &id).unwrap();
    }
    connection
}

fn insert_entries(tmp: &TempDir, source_id: &str, documents: &[SearchFileDocument]) {
    let id = DataSourceId(source_id.to_string());
    let connection = crate::source_db::open_source_db(tmp.path(), &id).unwrap();
    for document in documents {
        connection
            .execute(
                "INSERT INTO file_entries
                 (id, data_source_id, path, name, entry_type, size, ext, modified_at,
                  deleted, hidden, system, encrypted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    document.file_id,
                    source_id,
                    document.path,
                    document.name,
                    document.entry_type,
                    document.size,
                    (!document.extension.is_empty()).then_some(document.extension.clone()),
                    document.modified_at.map(|value| {
                        chrono::DateTime::<Utc>::from_timestamp_millis(value)
                            .unwrap()
                            .to_rfc3339()
                    }),
                    i32::from(document.deleted),
                    i32::from(document.hidden),
                    i32::from(document.system),
                    i32::from(document.encrypted),
                ],
            )
            .unwrap();
    }
    let index_dir = tmp.path().join("sources").join(source_id).join("index");
    let index = search::SearchIndex::create(&index_dir).unwrap();
    let mut writer = index.metadata_writer().unwrap();
    writer.add_documents(documents).unwrap();
    writer.commit().unwrap();
}

fn request(query: &str, limit: u32) -> SearchFilesRequest {
    SearchFilesRequest {
        query: query.to_string(),
        match_path: false,
        entry_type: SearchEntryTypeDto::Any,
        extensions: Vec::new(),
        data_source_ids: Vec::new(),
        sort_key: SearchSortKeyDto::Name,
        sort_direction: FileSortDirectionDto::Asc,
        offset: 0,
        limit,
        cursor: None,
    }
}

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

#[test]
fn file_search_indexes_directories_zero_byte_and_large_metadata_without_reading_content() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case(&tmp, &["ds-1"]);
    let mut large = document("large", "archive.7z", "/Downloads/archive.7z", "file");
    large.size = Some(3 * 1024 * 1024 * 1024);
    let documents = vec![
        document("empty", "report.txt", "/Downloads/report.txt", "file"),
        document("dir", "report-data", "/Downloads/report-data", "directory"),
        large,
    ];
    insert_entries(&tmp, "ds-1", &documents);

    let page = search_files_for_case(
        &case_conn,
        tmp.path(),
        &CaseId("case-1".to_string()),
        &request("report", 20),
    )
    .unwrap();

    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 2);
    assert!(page.items.iter().any(|item| item.name == "report.txt"));
    assert!(page.items.iter().any(|item| item.entry_type == "directory"));
    assert!(page.coverage.complete);
    assert_eq!(page.coverage.expected_entry_count, 3);
    assert_eq!(page.coverage.indexed_entry_count, 3);
}

#[test]
fn file_search_cursor_merges_sources_by_name_without_duplicates() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case(&tmp, &["ds-1", "ds-2"]);
    insert_entries(
        &tmp,
        "ds-1",
        &[
            document("one", "alpha.txt", "/alpha.txt", "file"),
            document("two", "charlie.txt", "/charlie.txt", "file"),
        ],
    );
    insert_entries(
        &tmp,
        "ds-2",
        &[
            document("one", "bravo.txt", "/bravo.txt", "file"),
            document("two", "delta.txt", "/delta.txt", "file"),
        ],
    );

    let mut current = request("", 2);
    let first = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &CaseId("case-1".to_string()),
        &current,
    )
    .unwrap();
    let mut names = first
        .items
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    current.cursor = first.next_cursor;
    let second = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &CaseId("case-1".to_string()),
        &current,
    )
    .unwrap();
    names.extend(second.items.into_iter().map(|item| item.name));

    assert_eq!(
        names,
        vec!["alpha.txt", "bravo.txt", "charlie.txt", "delta.txt"]
    );
}

#[test]
fn file_search_cursor_keeps_descending_ties_stable_across_sources() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case(&tmp, &["ds-1", "ds-2"]);
    insert_entries(
        &tmp,
        "ds-1",
        &[
            document("zeta", "zeta.txt", "/zeta.txt", "file"),
            document("same", "same.txt", "/one/same.txt", "file"),
        ],
    );
    insert_entries(
        &tmp,
        "ds-2",
        &[
            document("same", "same.txt", "/two/same.txt", "file"),
            document("alpha", "alpha.txt", "/alpha.txt", "file"),
        ],
    );

    let mut current = request("", 1);
    current.sort_direction = FileSortDirectionDto::Desc;
    let mut observed = Vec::new();
    loop {
        let page = search_files_for_case_cursor(
            &case_conn,
            tmp.path(),
            &CaseId("case-1".to_string()),
            &current,
        )
        .unwrap();
        observed.extend(
            page.items
                .into_iter()
                .map(|item| (item.name, item.data_source_id)),
        );
        let Some(cursor) = page.next_cursor else {
            break;
        };
        current.cursor = Some(cursor);
    }

    assert_eq!(
        observed,
        vec![
            ("zeta.txt".to_string(), "ds-1".to_string()),
            ("same.txt".to_string(), "ds-1".to_string()),
            ("same.txt".to_string(), "ds-2".to_string()),
            ("alpha.txt".to_string(), "ds-2".to_string()),
        ]
    );
}

#[test]
fn file_search_cursor_rejects_index_changes() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case(&tmp, &["ds-1"]);
    insert_entries(
        &tmp,
        "ds-1",
        &[
            document("one", "alpha.txt", "/alpha.txt", "file"),
            document("two", "bravo.txt", "/bravo.txt", "file"),
        ],
    );
    let mut first_request = request("", 1);
    let first = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &CaseId("case-1".to_string()),
        &first_request,
    )
    .unwrap();
    let cursor = first.next_cursor.unwrap();
    let index_dir = tmp.path().join("sources/ds-1/index");
    let index = search::SearchIndex::open(&index_dir).unwrap();
    let mut writer = index.metadata_writer().unwrap();
    writer
        .add_documents(&[document("three", "charlie.txt", "/charlie.txt", "file")])
        .unwrap();
    writer.commit().unwrap();
    first_request.cursor = Some(cursor);
    assert!(search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &CaseId("case-1".to_string()),
        &first_request,
    )
    .is_err());
}
