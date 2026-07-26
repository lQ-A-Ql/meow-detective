use super::*;
use crate::extractor::ExtractedText;
use tantivy::doc;

#[test]
fn rank_page_is_stable_and_materializes_only_the_selected_hit() {
    let temp = tempfile::TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();
    let texts = ["file-c", "file-a", "file-b"]
        .into_iter()
        .map(|file_id| ExtractedText {
            file_id: file_id.to_string(),
            content: "shared query token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 18,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| (text.file_id.clone(), format!("/{}.txt", text.file_id)))
        .collect::<Vec<_>>();
    index.index_documents(&texts, &paths).unwrap();

    let session = index.query_session("query").unwrap();
    let page = session.rank_page(1, 1).unwrap();

    assert_eq!(page.total_count, 3);
    assert_eq!(page.hits.len(), 1);
    assert_eq!(page.hits[0].file_id(), "file-b");
    let hit = session
        .materialize(page.hits.into_iter().next().unwrap())
        .unwrap();
    assert_eq!(hit.file_id, "file-b");
    assert_eq!(hit.path, "/file-b.txt");
    assert!(!hit.snippets.is_empty());
}

#[test]
fn zero_limit_rank_page_returns_the_exact_count() {
    let temp = tempfile::TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();
    index
        .index_documents(
            &[ExtractedText {
                file_id: "file-1".to_string(),
                content: "count token".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 11,
            }],
            &[("file-1".to_string(), "/file-1.txt".to_string())],
        )
        .unwrap();

    let page = index
        .query_session("count")
        .unwrap()
        .rank_page(0, 0)
        .unwrap();

    assert_eq!(page.total_count, 1);
    assert!(page.hits.is_empty());
}

#[test]
fn search_after_pages_do_not_replay_prior_hits() {
    let temp = tempfile::TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();
    let texts = (0..37)
        .rev()
        .map(|index| ExtractedText {
            file_id: format!("file-{index:03}"),
            content: "shared cursor token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 19,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| (text.file_id.clone(), format!("/{}.txt", text.file_id)))
        .collect::<Vec<_>>();
    index.index_documents(&texts, &paths).unwrap();

    let session = index.query_session("cursor").unwrap();
    let mut after = None;
    let mut ids = Vec::new();
    loop {
        let page = session.rank_after(after.as_ref(), 7).unwrap();
        if page.hits.is_empty() {
            break;
        }
        after = page.hits.last().map(SearchRankedHit::after_key);
        ids.extend(page.hits.into_iter().map(|hit| hit.file_id().to_string()));
    }

    assert_eq!(ids.len(), 37);
    assert_eq!(ids.first().map(String::as_str), Some("file-000"));
    assert_eq!(ids.last().map(String::as_str), Some("file-036"));
    let unique = ids.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), ids.len());
}

#[test]
fn search_after_rejects_hits_before_the_exact_score_and_id_key() {
    let temp = tempfile::TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();
    let texts = ["file-c", "file-a", "file-b"]
        .into_iter()
        .map(|file_id| ExtractedText {
            file_id: file_id.to_string(),
            content: "same score token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 16,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| (text.file_id.clone(), format!("/{}.txt", text.file_id)))
        .collect::<Vec<_>>();
    index.index_documents(&texts, &paths).unwrap();

    let session = index.query_session("score").unwrap();
    let first = session.rank_after(None, 2).unwrap();
    let after = first.hits.last().unwrap().after_key();
    assert_eq!(
        first
            .hits
            .iter()
            .map(SearchRankedHit::file_id)
            .collect::<Vec<_>>(),
        vec!["file-a", "file-b"]
    );

    let second = session.rank_after(Some(&after), 2).unwrap();
    assert_eq!(second.hits.len(), 1);
    assert_eq!(second.hits[0].file_id(), "file-c");
}

#[test]
fn query_session_exposes_the_commit_snapshot_opstamp() {
    let temp = tempfile::TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();
    index
        .index_documents(
            &[ExtractedText {
                file_id: "file-1".to_string(),
                content: "snapshot token".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 14,
            }],
            &[("file-1".to_string(), "/file-1.txt".to_string())],
        )
        .unwrap();

    let session = index.query_session("snapshot").unwrap();

    assert_eq!(
        session.snapshot_opstamp(),
        index.snapshot_opstamp().unwrap()
    );
}

#[test]
fn search_after_rejects_documents_without_file_id() {
    let temp = tempfile::TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();
    let path_field = index.schema.get_field("path").unwrap();
    let content_field = index.schema.get_field("content").unwrap();
    let name_field = index.schema.get_field("name").unwrap();
    let mut writer = index.index.writer(15_000_000).unwrap();
    writer
        .add_document(doc!(
            path_field => "/missing-id.txt",
            content_field => "malformed cursor document",
            name_field => "missing-id.txt",
        ))
        .unwrap();
    writer.commit().unwrap();

    let error = index
        .query_session("malformed")
        .unwrap()
        .rank_after(None, 10)
        .unwrap_err();

    assert!(matches!(error, IndexError::Schema(_)));
    assert!(error.to_string().contains("exactly one UTF-8 file_id"));
}
