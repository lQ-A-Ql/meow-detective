use crate::indexer::tantivy_writer::{SearchHighlight, SearchSnippet};

const MAX_HIGHLIGHT_CONTENT_BYTES: usize = 256 * 1024;
const MAX_SNIPPET_BYTES: usize = 512;
const MAX_SNIPPETS: usize = 5;
const SNIPPET_RADIUS: usize = 60;

pub fn highlight(content: &str, query: &str) -> Vec<SearchSnippet> {
    let content = capped_content(content);
    let lower_content = content.to_lowercase();
    let lower_query = query.to_lowercase();
    let terms: Vec<&str> = lower_query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect();

    if terms.is_empty() {
        return Vec::new();
    }

    let mut positions: Vec<usize> = Vec::new();
    for term in &terms {
        let mut start = 0usize;
        while let Some(pos) = lower_content[start..].find(*term) {
            let abs = start + pos;
            positions.push(abs);
            start = abs + term.len();
        }
    }
    positions.sort();
    positions.dedup();

    if positions.is_empty() {
        return Vec::new();
    }

    let mut snippets = Vec::new();
    let mut cluster_start = positions[0];

    for i in 1..positions.len() {
        if snippets.len() >= MAX_SNIPPETS {
            return snippets;
        }
        if positions[i] - positions[i - 1] > SNIPPET_RADIUS * 2 {
            snippets.push(build_snippet(
                content,
                cluster_start,
                positions[i - 1],
                &terms,
            ));
            cluster_start = positions[i];
        }
    }
    // Safety: positions is non-empty (checked above)
    if snippets.len() < MAX_SNIPPETS {
        if let Some(&last_pos) = positions.last() {
            snippets.push(build_snippet(content, cluster_start, last_pos, &terms));
        }
    }

    snippets
}

fn capped_content(content: &str) -> &str {
    cap_at_char_boundary(content, MAX_HIGHLIGHT_CONTENT_BYTES)
}

fn cap_at_char_boundary(content: &str, max_bytes: usize) -> &str {
    if content.len() <= max_bytes {
        return content;
    }

    let end = floor_char_boundary(content, max_bytes);
    &content[..end]
}

/// Move `index` down to the nearest valid UTF-8 char boundary (`<= index`).
fn floor_char_boundary(content: &str, index: usize) -> usize {
    let mut idx = index.min(content.len());
    while idx > 0 && !content.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn build_snippet(
    content: &str,
    first_hit: usize,
    last_hit: usize,
    terms: &[&str],
) -> SearchSnippet {
    let start = first_hit.saturating_sub(SNIPPET_RADIUS);
    let end = (last_hit + terms.iter().map(|t| t.len()).max().unwrap_or(0) + SNIPPET_RADIUS)
        .min(content.len());

    // Snap the byte window to UTF-8 char boundaries so slicing never splits a
    // multi-byte character.
    let start = floor_char_boundary(content, start);
    let end = floor_char_boundary(content, end);

    let text = cap_at_char_boundary(&content[start..end], MAX_SNIPPET_BYTES);

    let lower_text = text.to_lowercase();
    let mut highlights = Vec::new();

    for term in terms {
        let mut search_start = 0usize;
        while let Some(pos) = lower_text[search_start..].find(*term) {
            let abs = search_start + pos;
            highlights.push(SearchHighlight {
                start: abs as u32,
                end: (abs + term.len()) as u32,
            });
            search_start = abs + term.len();
        }
    }

    highlights.sort_by_key(|h| h.start);

    SearchSnippet {
        text: text.to_string(),
        highlights,
    }
}

#[cfg(test)]
mod tests {
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
}
