use super::*;

#[test]
fn highlight_limits_snippets_for_large_repeated_content() {
    let content = (0..100)
        .map(|i| format!("match-{i:03} credential {}", "x".repeat(200)))
        .collect::<Vec<_>>()
        .join("\n");

    let snippets = highlight(&content, "credential");

    assert_eq!(snippets.len(), 5);
    assert!(snippets.iter().all(|snippet| snippet.text.len() <= 130));
    assert!(snippets
        .iter()
        .all(|snippet| snippet.text.contains("credential")));
    assert!(snippets
        .iter()
        .all(|snippet| snippet.highlights.iter().any(|h| h.start < h.end)));
}

#[test]
fn highlight_limits_dense_match_snippet_text() {
    let content = "credential ".repeat(10_000);

    let snippets = highlight(&content, "credential");

    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].text.len() <= MAX_SNIPPET_BYTES);
    assert!(snippets[0].text.contains("credential"));
    assert!(snippets[0]
        .highlights
        .iter()
        .any(|highlight| highlight.start < highlight.end));
}

#[test]
fn highlight_caps_scanned_content_at_indexing_budget() {
    let content = format!(
        "{} credential",
        "a".repeat(MAX_HIGHLIGHT_CONTENT_BYTES + 1024)
    );

    let snippets = highlight(&content, "credential");

    assert!(snippets.is_empty());
}

#[test]
fn highlight_empty_content() {
    let snippets = highlight("", "test");
    assert!(snippets.is_empty());
}

#[test]
fn highlight_empty_query() {
    let snippets = highlight("hello world", "");
    assert!(snippets.is_empty());
}

#[test]
fn highlight_whitespace_only_query() {
    let snippets = highlight("hello world", "   ");
    assert!(snippets.is_empty());
}

#[test]
fn highlight_no_match() {
    let snippets = highlight("hello world", "xyz");
    assert!(snippets.is_empty());
}

#[test]
fn highlight_single_match() {
    let snippets = highlight("hello world", "world");
    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].text.contains("world"));
    assert!(!snippets[0].highlights.is_empty());
}

#[test]
fn highlight_multiple_terms() {
    let snippets = highlight("the quick brown fox jumps over the lazy dog", "quick fox");
    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].text.contains("quick"));
    assert!(snippets[0].text.contains("fox"));
}

#[test]
fn highlight_case_insensitive() {
    let snippets = highlight("Hello World", "hello");
    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].text.contains("Hello"));
}

#[test]
fn highlight_highlights_contain_correct_offsets() {
    let content = "abc def ghi jkl mno pqr abc";
    let snippets = highlight(content, "abc");
    // "abc" at position 0 and "abc" at position 24 — far enough apart for 2 clusters
    assert!(!snippets.is_empty());
    // First snippet should have highlight with start < end
    assert!(!snippets[0].highlights.is_empty());
    assert!(snippets[0].highlights[0].start < snippets[0].highlights[0].end);
}

#[test]
fn highlight_utf8_content() {
    let content = "这是一段中文内容，包含关键词 test";
    let snippets = highlight(content, "test");
    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].text.contains("test"));
}

#[test]
fn highlight_special_characters_in_content() {
    let content = "path: C:\\Users\\test\\file.txt <script>alert(1)</script>";
    let snippets = highlight(content, "test");
    assert!(!snippets.is_empty());
}

#[test]
fn highlight_query_with_quotes() {
    let snippets = highlight("say \"hello\" world", "hello");
    assert_eq!(snippets.len(), 1);
}

#[test]
fn highlight_snippets_sorted_by_position() {
    let content = "a b c match d e f match g h i";
    let snippets = highlight(content, "match");
    // All snippets should be in document order
    for window in snippets.windows(2) {
        // Each snippet's text starts before the next one's in the original content
        let first_start = content.find(&window[0].text).unwrap_or(0);
        let second_start = content.find(&window[1].text).unwrap_or(0);
        assert!(first_start <= second_start);
    }
}
