use search::{extract_text, SearchIndex};
use tempfile::TempDir;

#[test]
fn create_and_search_index() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("test-index");

    let text1 = extract_text(
        b"forensics analysis tool".as_slice(),
        "f1",
        Some("text/plain"),
    );
    let text2 = extract_text(
        b"windows registry artifact".as_slice(),
        "f2",
        Some("text/plain"),
    );
    let texts = vec![text1, text2];
    let paths = vec![
        ("f1".to_string(), "/case/evidence/file1.txt".to_string()),
        ("f2".to_string(), "/case/evidence/file2.txt".to_string()),
    ];

    let index = SearchIndex::create(&index_dir).unwrap();
    let count = index.index_documents(&texts, &paths).unwrap();
    assert_eq!(count, 2);

    let result = index.search("forensics", 10).unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.total_count, 1);
    assert_eq!(result.hits[0].file_id, "f1");

    let result = index.search("registry", 10).unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.total_count, 1);
    assert_eq!(result.hits[0].file_id, "f2");
}

#[test]
fn search_no_results() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("empty-index");

    let text = extract_text(b"some data".as_slice(), "f1", Some("text/plain"));
    let index = SearchIndex::create(&index_dir).unwrap();
    index
        .index_documents(&[text], &[("f1".to_string(), "/f".to_string())])
        .unwrap();

    let result = index.search("nonexistent", 10).unwrap();
    assert!(result.hits.is_empty());
    assert_eq!(result.total_count, 0);
}

#[test]
fn search_many_results_respects_limit_and_total_count() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("many-results");

    let texts: Vec<_> = (0..25)
        .map(|i| {
            extract_text(
                format!("forensics document {i}").as_bytes(),
                &format!("f{i}"),
                Some("text/plain"),
            )
        })
        .collect();
    let paths: Vec<_> = (0..25)
        .map(|i| (format!("f{i}"), format!("/case/evidence/file{i}.txt")))
        .collect();

    let index = SearchIndex::create(&index_dir).unwrap();
    let count = index.index_documents(&texts, &paths).unwrap();
    assert_eq!(count, 25);

    let result = index.search("forensics", 10).unwrap();
    assert_eq!(result.hits.len(), 10);
    assert_eq!(result.total_count, 25);
    assert!(result.hits.iter().all(|hit| !hit.snippets.is_empty()));
}

#[test]
fn reopen_index() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("reopen");

    let text = extract_text(b"critical evidence".as_slice(), "f1", Some("text/plain"));
    {
        let index = SearchIndex::create(&index_dir).unwrap();
        index
            .index_documents(&[text], &[("f1".to_string(), "/f".to_string())])
            .unwrap();
    }

    let index = SearchIndex::open(&index_dir).unwrap();
    let result = index.search("evidence", 10).unwrap();
    assert_eq!(result.hits.len(), 1);
}

#[test]
fn search_returns_highlights() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("highlight-index");

    let text = extract_text(
        b"The forensic examiner found credential data in the registry".as_slice(),
        "f1",
        Some("text/plain"),
    );
    let index = SearchIndex::create(&index_dir).unwrap();
    index
        .index_documents(
            &[text],
            &[("f1".to_string(), "/evidence/reg.txt".to_string())],
        )
        .unwrap();

    let result = index.search("credential", 10).unwrap();
    assert_eq!(result.hits.len(), 1);
    assert!(!result.hits[0].snippets.is_empty());
    let snippet = &result.hits[0].snippets[0];
    assert!(!snippet.text.is_empty(), "snippet text should not be empty");
    assert!(
        !snippet.highlights.is_empty(),
        "highlights should not be empty"
    );
    assert!(snippet.text.to_lowercase().contains("credential"));
}

#[test]
fn search_highlights_large_content_with_bounded_snippets() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("large-highlight-index");

    let content = (0..100)
        .map(|i| format!("record-{i:03} credential {}", "x".repeat(200)))
        .collect::<Vec<_>>()
        .join("\n");
    let text = extract_text(content.as_bytes(), "f1", Some("text/plain"));
    let index = SearchIndex::create(&index_dir).unwrap();
    index
        .index_documents(
            &[text],
            &[("f1".to_string(), "/evidence/large.txt".to_string())],
        )
        .unwrap();

    let result = index.search("credential", 10).unwrap();
    assert_eq!(result.hits.len(), 1);

    let snippets = &result.hits[0].snippets;
    assert_eq!(snippets.len(), 5);
    assert!(snippets.iter().all(|snippet| snippet.text.len() <= 130));
    assert!(snippets
        .iter()
        .all(|snippet| snippet.text.contains("credential")));
    assert!(snippets.iter().all(|snippet| {
        snippet
            .highlights
            .iter()
            .any(|highlight| highlight.start < highlight.end)
    }));
}

#[test]
fn search_highlights_dense_large_content_with_bounded_snippet_text() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("dense-large-highlight-index");

    let content = "credential ".repeat(10_000);
    let text = extract_text(content.as_bytes(), "f1", Some("text/plain"));
    let index = SearchIndex::create(&index_dir).unwrap();
    index
        .index_documents(
            &[text],
            &[("f1".to_string(), "/evidence/dense-large.txt".to_string())],
        )
        .unwrap();

    let result = index.search("credential", 10).unwrap();
    assert_eq!(result.hits.len(), 1);

    let snippets = &result.hits[0].snippets;
    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].text.len() <= 512);
    assert!(snippets[0].text.contains("credential"));
    assert!(snippets[0]
        .highlights
        .iter()
        .any(|highlight| highlight.start < highlight.end));
}

#[test]
fn search_multi_term_query() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("multi-term");

    let text1 = extract_text(
        b"forensics analysis tool for windows".as_slice(),
        "f1",
        Some("text/plain"),
    );
    let text2 = extract_text(b"simple text editor".as_slice(), "f2", Some("text/plain"));
    let index = SearchIndex::create(&index_dir).unwrap();
    index
        .index_documents(
            &[text1, text2],
            &[
                ("f1".to_string(), "/f1.txt".to_string()),
                ("f2".to_string(), "/f2.txt".to_string()),
            ],
        )
        .unwrap();

    let result = index.search("forensics windows", 10).unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].file_id, "f1");
}
