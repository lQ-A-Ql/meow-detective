//! Helpers shared by more than one email container format.

const BODY_PREVIEW_MAX_LEN: usize = 500;
const BODY_PREVIEW_MAX_LINES: usize = 8;

pub(super) fn build_body_preview(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(BODY_PREVIEW_MAX_LINES)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(BODY_PREVIEW_MAX_LEN)
        .collect()
}
