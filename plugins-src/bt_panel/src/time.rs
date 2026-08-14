//! BT panel timestamp handling.
//!
//! The panel stores `addtime`-style columns as server-local wall-clock text
//! (`YYYY-MM-DD HH:MM:SS`); the timezone is not recorded. The payload carries
//! the naive local time and declares `timesAreLocal`, so the host converts
//! with the resolved host timezone before persistence (design doc §4).

use chrono::NaiveDateTime;

/// Formats seen in the wild: canonical panel `addtime`, ISO-8601 with `T`,
/// and the all-zero placeholder used by factory-default rows.
const FORMATS: &[&str] = &["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"];

/// Parse a panel timestamp into naive ISO 8601 (no zone suffix — the host
/// applies the resolved timezone). Returns `None` for empty, placeholder
/// (`0000-00-00 ...`) and unparsable values — the caller then omits the
/// field entirely.
pub fn to_local_iso(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with("0000-00-00") {
        return None;
    }
    for format in FORMATS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Some(naive.format("%Y-%m-%dT%H:%M:%S").to_string());
        }
    }
    None
}

pub const TIMEZONE_WARNING: &str =
    "panel timestamps are server-local wall clock; the host converts them with the inferred timezone";
