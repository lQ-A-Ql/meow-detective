use crate::indexer::tantivy_writer::{SearchHighlight, SearchSnippet};

const SNIPPET_RADIUS: usize = 60;

pub fn highlight(content: &str, query: &str) -> Vec<SearchSnippet> {
    let lower_content = content.to_lowercase();
    let lower_query = query.to_lowercase();
    let terms: Vec<&str> = lower_query.split_whitespace().collect();

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
        if positions[i] - positions[i - 1] > SNIPPET_RADIUS * 2 {
            snippets.push(build_snippet(content, cluster_start, positions[i - 1], &terms));
            cluster_start = positions[i];
        }
    }
    snippets.push(build_snippet(content, cluster_start, *positions.last().unwrap(), &terms));

    snippets
}

fn build_snippet(content: &str, first_hit: usize, last_hit: usize, terms: &[&str]) -> SearchSnippet {
    let start = first_hit.saturating_sub(SNIPPET_RADIUS);
    let end = (last_hit + terms.iter().map(|t| t.len()).max().unwrap_or(0) + SNIPPET_RADIUS)
        .min(content.len());

    let text = &content[start..end];

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
