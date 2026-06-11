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
mod tests {
    use super::*;

    #[test]
    fn html_escape_empty_string() {
        assert_eq!(html_escape(""), "");
    }

    #[test]
    fn html_escape_no_special_chars() {
        assert_eq!(html_escape("hello world"), "hello world");
    }

    #[test]
    fn html_escape_escapes_angle_brackets() {
        assert_eq!(html_escape("<div>"), "&lt;div&gt;");
    }

    #[test]
    fn html_escape_escapes_ampersand() {
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn html_escape_escapes_quotes() {
        assert_eq!(html_escape(r#"say "hello""#), "say &quot;hello&quot;");
        assert_eq!(html_escape("it's"), "it&#x27;s");
    }

    #[test]
    fn html_escape_escapes_all_special_chars() {
        let input = r#"<script>alert("xss")</script>"#;
        let escaped = html_escape(input);
        assert!(escaped.contains("&lt;script&gt;"));
        assert!(escaped.contains("&quot;xss&quot;"));
        assert!(!escaped.contains("<script>"));
    }

    #[test]
    fn html_escape_preserves_normal_text() {
        let input = "File: C:\\Users\\test\\document.txt";
        assert_eq!(html_escape(input), input);
    }

    #[test]
    fn html_escape_unicode_passthrough() {
        let input = "中文测试 🔍";
        assert_eq!(html_escape(input), input);
    }
}
