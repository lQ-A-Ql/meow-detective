use search::{extract_text, SearchIndex};
use tempfile::TempDir;

#[test]
fn create_and_search_index() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("test-index");

    let text1 = extract_text(b"forensics analysis tool".as_slice(), "f1", Some("text/plain"));
    let text2 = extract_text(b"windows registry artifact".as_slice(), "f2", Some("text/plain"));
    let texts = vec![text1, text2];
    let paths = vec![
        ("f1".to_string(), "/case/evidence/file1.txt".to_string()),
        ("f2".to_string(), "/case/evidence/file2.txt".to_string()),
    ];

    let index = SearchIndex::create(&index_dir).unwrap();
    let count = index.index_documents(&texts, &paths).unwrap();
    assert_eq!(count, 2);

    let hits = index.search("forensics", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].file_id, "f1");

    let hits = index.search("registry", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].file_id, "f2");
}

#[test]
fn search_no_results() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("empty-index");

    let text = extract_text(b"some data".as_slice(), "f1", Some("text/plain"));
    let index = SearchIndex::create(&index_dir).unwrap();
    index.index_documents(&[text], &[("f1".to_string(), "/f".to_string())]).unwrap();

    let hits = index.search("nonexistent", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn reopen_index() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("reopen");

    let text = extract_text(b"critical evidence".as_slice(), "f1", Some("text/plain"));
    {
        let index = SearchIndex::create(&index_dir).unwrap();
        index.index_documents(&[text], &[("f1".to_string(), "/f".to_string())]).unwrap();
    }

    let index = SearchIndex::open(&index_dir).unwrap();
    let hits = index.search("evidence", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_returns_highlights() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("highlight-index");

    let text = extract_text(
        b"The forensic examiner found credential data in the registry".as_slice(),
        "f1", Some("text/plain"),
    );
    let index = SearchIndex::create(&index_dir).unwrap();
    index.index_documents(&[text], &[("f1".to_string(), "/evidence/reg.txt".to_string())]).unwrap();

    let hits = index.search("credential", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(!hits[0].snippets.is_empty());
    let snippet = &hits[0].snippets[0];
    assert!(!snippet.text.is_empty(), "snippet text should not be empty");
    assert!(!snippet.highlights.is_empty(), "highlights should not be empty");
    assert!(snippet.text.to_lowercase().contains("credential"));
}

#[test]
fn search_multi_term_query() {
    let tmp = TempDir::new().unwrap();
    let index_dir = tmp.path().join("multi-term");

    let text1 = extract_text(b"forensics analysis tool for windows".as_slice(), "f1", Some("text/plain"));
    let text2 = extract_text(b"simple text editor".as_slice(), "f2", Some("text/plain"));
    let index = SearchIndex::create(&index_dir).unwrap();
    index.index_documents(&[text1, text2], &[
        ("f1".to_string(), "/f1.txt".to_string()),
        ("f2".to_string(), "/f2.txt".to_string()),
    ]).unwrap();

    let hits = index.search("forensics windows", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].file_id, "f1");
}
