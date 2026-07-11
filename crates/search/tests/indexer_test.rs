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
fn reindex_same_file_id_replaces_existing_document() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("reindex-upsert");

    let index = SearchIndex::create(&index_dir).unwrap();
    let first = extract_text(b"old-token credential".as_slice(), "f1", Some("text/plain"));
    index
        .index_documents(
            &[first],
            &[("f1".to_string(), "/case/evidence/file.txt".to_string())],
        )
        .unwrap();

    let replacement = extract_text(b"new-token credential".as_slice(), "f1", Some("text/plain"));
    index
        .index_documents(
            &[replacement],
            &[("f1".to_string(), "/case/evidence/file.txt".to_string())],
        )
        .unwrap();

    let old_result = index.search("old-token", 10).unwrap();
    assert!(old_result.hits.is_empty());
    assert_eq!(old_result.total_count, 0);

    let new_result = index.search("new-token", 10).unwrap();
    assert_eq!(new_result.hits.len(), 1);
    assert_eq!(new_result.total_count, 1);
    assert_eq!(new_result.hits[0].file_id, "f1");
}

#[test]
fn binary_documents_are_not_indexed_or_left_stale() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("binary-skip");

    let index = SearchIndex::create(&index_dir).unwrap();
    let text = extract_text(
        b"credential searchable".as_slice(),
        "bin1",
        Some("text/plain"),
    );
    index
        .index_documents(
            &[text],
            &[("bin1".to_string(), "/evidence/blob.txt".to_string())],
        )
        .unwrap();
    assert_eq!(index.search("credential", 10).unwrap().total_count, 1);

    let binary = extract_text(
        b"credential in binary should not index".as_slice(),
        "bin1",
        Some("application/octet-stream"),
    );
    let count = index
        .index_documents(
            &[binary],
            &[("bin1".to_string(), "/evidence/blob.bin".to_string())],
        )
        .unwrap();

    assert_eq!(count, 0);
    let result = index.search("credential", 10).unwrap();
    assert!(result.hits.is_empty());
    assert_eq!(result.total_count, 0);
}

#[test]
fn highlight_offsets_remain_inside_multibyte_snippet_boundaries() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("unicode-highlight-boundaries");

    let content = format!("{} credential {}", "é".repeat(80), "😀".repeat(80));
    let text = extract_text(content.as_bytes(), "f1", Some("text/plain"));
    let index = SearchIndex::create(&index_dir).unwrap();
    index
        .index_documents(
            &[text],
            &[("f1".to_string(), "/evidence/unicode.txt".to_string())],
        )
        .unwrap();

    let result = index.search("credential", 10).unwrap();
    assert_eq!(result.hits.len(), 1);
    let snippet = &result.hits[0].snippets[0];
    assert!(snippet.text.is_char_boundary(snippet.text.len()));
    for highlight in &snippet.highlights {
        let start = highlight.start as usize;
        let end = highlight.end as usize;
        assert!(start < end);
        assert!(end <= snippet.text.len());
        assert!(snippet.text.is_char_boundary(start));
        assert!(snippet.text.is_char_boundary(end));
        assert_eq!(&snippet.text[start..end].to_lowercase(), "credential");
    }
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

    let result = index.search("missing", 10).unwrap();
    assert_eq!(result.hits.len(), 0);
    assert_eq!(result.total_count, 0);
}

#[test]
fn search_respects_limit() {
    let dir = tempfile::tempdir().expect("create search fixture");
    let index_path = dir.path().join("test_index");
    let index = search::SearchIndex::create(&index_path).expect("create search index");
    let texts = (0..20)
        .map(|index| search::ExtractedText {
            file_id: format!("f{index}"),
            content: format!("document number {index} with test content"),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 50,
        })
        .collect::<Vec<_>>();
    let paths = (0..20)
        .map(|index| (format!("f{index}"), format!("/docs/doc{index}.txt")))
        .collect::<Vec<_>>();

    index
        .index_documents(&texts, &paths)
        .expect("index fixture");
    let result = index.search("test", 5).expect("search fixture");

    assert!(result.hits.len() <= 5);
}
