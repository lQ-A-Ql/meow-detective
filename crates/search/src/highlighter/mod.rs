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

    let mut end = max_bytes;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    &content[..end]
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
}
