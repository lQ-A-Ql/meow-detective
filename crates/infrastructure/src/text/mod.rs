//! Text extraction and processing.

/// Escape special HTML characters to prevent XSS in report output.
///
/// Replaces `<`, `>`, `&`, `"`, `'` with their HTML entity equivalents.
pub fn html_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
#[path = "../../tests/unit/text.rs"]
mod tests;
