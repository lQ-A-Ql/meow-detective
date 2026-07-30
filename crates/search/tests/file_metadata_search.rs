use std::collections::HashSet;

use search::{
    FileEntryTypeFilter, FileSearchAfterKey, FileSearchOptions, FileSearchSortDirection,
    FileSearchSortField, SearchFileDocument, SearchIndex,
};

fn document(
    id: &str,
    name: &str,
    path: &str,
    entry_type: &str,
    size: Option<u64>,
    modified_at: Option<i64>,
) -> SearchFileDocument {
    SearchFileDocument {
        file_id: id.to_string(),
        path: path.to_string(),
        name: name.to_string(),
        extension: name
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .unwrap_or_default()
            .to_string(),
        entry_type: entry_type.to_string(),
        size,
        modified_at,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
    }
}

fn query_ids(index: &SearchIndex, options: FileSearchOptions) -> Vec<String> {
    let session = index.file_query_session(&options).unwrap();
    session
        .rank_after(None, 100)
        .unwrap()
        .hits
        .into_iter()
        .map(|ranked| ranked.file_id().to_string())
        .collect()
}

#[test]
fn metadata_query_supports_unicode_short_terms_globs_paths_and_filters() {
    let directory = tempfile::tempdir().unwrap();
    let index = SearchIndex::create(directory.path()).unwrap();
    let mut writer = index.metadata_writer().unwrap();
    writer
        .add_documents(&[
            document(
                "chinese",
                "检材报告.txt",
                "/案件/下载/检材报告.txt",
                "file",
                Some(12),
                Some(10),
            ),
            document(
                "report",
                "Report.TXT",
                "/Users/alice/Documents/Report.TXT",
                "file",
                Some(32),
                Some(20),
            ),
            document(
                "path-only",
                "notes.log",
                "/Secret/Archive/notes.log",
                "file",
                Some(64),
                Some(30),
            ),
            document(
                "directory",
                "Reports",
                "/Users/alice/Reports",
                "directory",
                None,
                None,
            ),
        ])
        .unwrap();
    writer.commit().unwrap();

    for term in ["检", "检材", "材报告"] {
        assert_eq!(
            query_ids(
                &index,
                FileSearchOptions {
                    query: term.to_string(),
                    ..Default::default()
                },
            ),
            vec!["chinese"]
        );
    }
    assert_eq!(
        query_ids(
            &index,
            FileSearchOptions {
                query: "ＲＥＰＯＲＴ".to_string(),
                ..Default::default()
            },
        ),
        vec!["report", "directory"]
    );
    assert_eq!(
        query_ids(
            &index,
            FileSearchOptions {
                query: "rep?rt.*".to_string(),
                ..Default::default()
            },
        ),
        vec!["report"]
    );
    assert!(query_ids(
        &index,
        FileSearchOptions {
            query: "secret".to_string(),
            ..Default::default()
        },
    )
    .is_empty());
    assert_eq!(
        query_ids(
            &index,
            FileSearchOptions {
                query: "secret".to_string(),
                match_path: true,
                ..Default::default()
            },
        ),
        vec!["path-only"]
    );

    let filtered = query_ids(
        &index,
        FileSearchOptions {
            entry_type: FileEntryTypeFilter::File,
            extensions: vec![".TXT".to_string()],
            ..Default::default()
        },
    )
    .into_iter()
    .collect::<HashSet<_>>();
    assert_eq!(
        filtered,
        HashSet::from(["chinese".to_string(), "report".to_string()])
    );
    assert_eq!(
        query_ids(
            &index,
            FileSearchOptions {
                entry_type: FileEntryTypeFilter::Directory,
                ..Default::default()
            },
        ),
        vec!["directory"]
    );
}

#[test]
fn search_after_is_complete_and_stable_for_all_sorts_across_segments() {
    let directory = tempfile::tempdir().unwrap();
    let index = SearchIndex::create(directory.path()).unwrap();
    let documents = [
        document("id-08", "same.txt", "/b/same.txt", "file", None, None),
        document(
            "id-03",
            "alpha",
            "/z/alpha",
            "directory",
            Some(0),
            Some(-10),
        ),
        document("id-06", "same.txt", "/a/same.txt", "file", Some(7), Some(0)),
        document(
            "id-01",
            "bravo.bin",
            "/d/bravo.bin",
            "file",
            Some(7),
            Some(10),
        ),
        document("id-07", "echo.log", "/c/echo.log", "file", Some(99), None),
        document("id-04", "delta", "/e/delta", "directory", None, Some(10)),
        document(
            "id-02",
            "charlie.txt",
            "/f/charlie.txt",
            "file",
            Some(0),
            Some(-10),
        ),
        document(
            "id-05",
            "foxtrot",
            "/g/foxtrot",
            "directory",
            Some(1),
            Some(0),
        ),
    ];
    for batch in documents.chunks(4) {
        let mut writer = index.metadata_writer().unwrap();
        writer.add_documents(batch).unwrap();
        writer.commit().unwrap();
    }

    for field in [
        FileSearchSortField::Name,
        FileSearchSortField::Path,
        FileSearchSortField::Size,
        FileSearchSortField::ModifiedAt,
    ] {
        for direction in [FileSearchSortDirection::Asc, FileSearchSortDirection::Desc] {
            assert_complete_paging(&index, field, direction, documents.len());
        }
    }

    let session = index
        .file_query_session(&FileSearchOptions::default())
        .unwrap();
    let ranked = session.rank_after(None, documents.len()).unwrap();
    let hits = ranked
        .hits
        .into_iter()
        .map(|hit| session.materialize(hit).unwrap())
        .collect::<Vec<_>>();
    assert!(hits
        .iter()
        .any(|hit| hit.file_id == "id-08" && hit.size.is_none()));
    assert!(hits
        .iter()
        .any(|hit| hit.file_id == "id-08" && hit.modified_at.is_none()));
}

fn assert_complete_paging(
    index: &SearchIndex,
    field: FileSearchSortField,
    direction: FileSearchSortDirection,
    expected_count: usize,
) {
    let session = index
        .file_query_session(&FileSearchOptions {
            sort_field: field,
            sort_direction: direction,
            ..Default::default()
        })
        .unwrap();
    let mut after: Option<FileSearchAfterKey> = None;
    let mut ranks = Vec::new();
    loop {
        let page = session.rank_after(after.as_ref(), 2).unwrap();
        assert_eq!(page.total_count, expected_count as u64);
        if page.hits.is_empty() {
            break;
        }
        after = page.hits.last().map(|hit| hit.after_key());
        ranks.extend(
            page.hits
                .into_iter()
                .map(|hit| (hit.sort_value().to_string(), hit.file_id().to_string())),
        );
        assert!(
            ranks.len() <= expected_count,
            "search-after cursor repeated a hit"
        );
    }

    assert_eq!(ranks.len(), expected_count);
    assert_eq!(
        ranks.iter().map(|(_, id)| id).collect::<HashSet<_>>().len(),
        expected_count
    );
    assert!(ranks.windows(2).all(|pair| {
        let primary = match direction {
            FileSearchSortDirection::Asc => pair[0].0.cmp(&pair[1].0),
            FileSearchSortDirection::Desc => pair[1].0.cmp(&pair[0].0),
        };
        primary.is_lt() || (primary.is_eq() && pair[0].1 < pair[1].1)
    }));
}
