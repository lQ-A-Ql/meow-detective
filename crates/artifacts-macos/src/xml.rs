//! Small XML utilities shared across macOS artifact parsers.
//!
//! These helpers intentionally avoid pulling in a full XML dependency for the
//! simple line-oriented plist fragments that several parsers need to scan.

/// Extract content from an XML tag like `<tag>content</tag>`.
///
/// Returns `Some(String::new())` when the tag pair is present but empty.
pub(crate) fn extract_xml_tag_content(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    if let (Some(start), Some(end)) = (line.find(&open), line.find(&close)) {
        let content_start = start + open.len();
        if content_start < end {
            return Some(line[content_start..end].to_string());
        }
        return Some(String::new());
    }
    None
}
