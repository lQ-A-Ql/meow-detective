//! BT panel timestamp handling.
//!
//! The panel stores `addtime`-style columns as server-local wall-clock text
//! (`YYYY-MM-DD HH:MM:SS`); the timezone is not recorded. We cannot recover
//! it from the database alone, so timestamps are emitted as-is labelled UTC
//! and every database that produced at least one timestamp also produces a
//! warning stating the assumption (payload schema §4: distinguish
//! unparsed/absent/unsupported empties).

use chrono::NaiveDateTime;

/// Formats seen in the wild: canonical panel `addtime`, ISO-8601 with `T`,
/// and the all-zero placeholder used by factory-default rows.
const FORMATS: &[&str] = &["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"];

/// Parse a panel timestamp into RFC 3339 (UTC-labelled). Returns `None`
/// for empty, placeholder (`0000-00-00 ...`) and unparsable values — the
/// caller then omits the field entirely.
pub fn to_utc_iso(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with("0000-00-00") {
        return None;
    }
    for format in FORMATS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Some(naive.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        }
    }
    None
}

pub const TIMEZONE_WARNING: &str =
    "panel timestamps are server-local wall clock; emitted labelled as UTC (timezone unrecoverable from the database)";
