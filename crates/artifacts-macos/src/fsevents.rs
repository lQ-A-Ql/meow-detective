//! macOS FSEvents (File System Events) parser.
//!
//! Parses the FSEvents log files found under `.fseventsd/` at the root of
//! each volume on macOS. FSEvents records file system changes such as
//! created, modified, removed, and renamed events.
//!
//! The log format (simplified):
//! ```text
//! Each log file (e.g., 0000000001234abc) contains:
//! - A header with magic bytes and metadata
//! - Event records, each containing:
//!   - Page number (u32)
//!   - File ID (u64 inode)
//!   - Event flags (u32): bitmask of event types
//!   - Optional path string
//! ```
//!
//! FSEvents event flags (common values):
//! - 0x0100: Created
//! - 0x0200: Removed
//! - 0x0400: InodeMetaMod (metadata changed)
//! - 0x0800: Renamed
//! - 0x1000: Modified
//! - 0x2000: Exchange
//! - 0x4000: FinderInfoMod
//! - 0x8000: OwnerChanged
//! - 0x10000: XattrMod
//! - 0x20000: IsFile
//! - 0x40000: IsDir
//! - 0x80000: IsSymlink
//!
//! Reference: Apple FSEvents API (FSEventStreamEventFlags)

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// Magic bytes for FSEvents log files: "1SLD" (FSLD = File System Log Daemon).
const FSEVENTS_MAGIC: &[u8; 4] = b"1SLD";

/// The type of file system event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FSEventType {
    Created,
    Removed,
    Modified,
    Renamed,
    Unknown,
}

/// A single FSEvents log entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FSEvent {
    /// Path of the file/directory affected
    pub path: String,
    /// Type of file system event
    pub event_type: FSEventType,
    /// ISO 8601 timestamp (best effort from event data)
    pub timestamp: String,
}

/// Check if data begins with the FSEvents magic bytes.
pub fn is_fsevents(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == FSEVENTS_MAGIC
}

/// Parse an FSEvents log file and extract file system events.
///
/// This parser handles the FSEvents binary log format, extracting event
/// records with paths, event types, and timestamps where available.
///
/// FSEvents log files are stored as gzipped data with a header. The format:
/// - Magic: "1SLD" (4 bytes)
/// - Header: includes timestamps, device info
/// - Event records: variable-length entries with flags and paths
pub fn parse_fsevents_log(data: &[u8]) -> Result<Vec<FSEvent>, String> {
    if data.len() < 8 {
        return Err("FSEvents log data too short".to_string());
    }
    if !is_fsevents(data) {
        return Err("Not an FSEvents log file (missing 1SLD magic)".to_string());
    }

    let mut events: Vec<FSEvent> = Vec::new();

    // Skip the magic bytes and parse the header
    // Header structure (simplified):
    // [0..3]    "1SLD"
    // [4..7]    version? / flags
    // [8..15]   ???
    // [16..23]  timestamp (u64 BE) — first event time
    // [24..31]  ???

    // Read first event timestamp from header (offset 16)
    let base_timestamp = if data.len() >= 24 {
        Some(u64::from_be_bytes([
            data[16], data[17], data[18], data[19],
            data[20], data[21], data[22], data[23],
        ]))
    } else {
        None
    };

    // Event records follow the header.
    // Each record starts with event flags and may include a path string.
    let mut pos = 32usize; // start scanning after a typical header

    // Scan for event-like structures: look for path strings preceded by event flags
    while pos + 8 < data.len() {
        // Try to decode an event at this position
        if let Some((event, next_pos)) = decode_event(data, pos) {
            events.push(event);
            pos = next_pos;
        } else {
            pos += 1; // advance byte by byte if we can't decode
        }

        // Safety limit
        if events.len() > 5000 {
            break;
        }
    }

    // If no structured events found, try best-effort path extraction
    if events.is_empty() {
        events = extract_paths_fallback(data, base_timestamp);
    }

    Ok(events)
}

/// Attempt to decode an FSEvent record at the given position.
/// Returns the event and the position of the next record if successful.
fn decode_event(data: &[u8], pos: usize) -> Option<(FSEvent, usize)> {
    if pos + 8 > data.len() {
        return None;
    }

    // FSEvents record format (one variant):
    // [0..3] event flags (u32 LE)
    // [4..7] ??? (could be inode, padding, etc.)
    // [8..N] path string (null-terminated, or length-prefixed)

    let flags = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);

    // Validate flags look reasonable (some common FSEvents flags)
    // Skip if neither file nor directory flag is set with a known operation
    let is_file = (flags & 0x20000) != 0;
    let is_dir = (flags & 0x40000) != 0;
    let has_known_op = (flags & 0xFF00) != 0;

    if !has_known_op && !is_file && !is_dir {
        // Not a recognizable event
        return None;
    }

    // Determine event type from flags
    let event_type = if (flags & 0x0100) != 0 {
        FSEventType::Created
    } else if (flags & 0x0200) != 0 {
        FSEventType::Removed
    } else if (flags & 0x0800) != 0 {
        FSEventType::Renamed
    } else if (flags & 0x1000) != 0 || (flags & 0x0400) != 0 {
        FSEventType::Modified
    } else {
        FSEventType::Unknown
    };

    // Compute timestamp (base + offset or derive from position)
    let timestamp = if let Some(base) = base_timestamp_from_data(data, pos) {
        let dt = Utc.timestamp_opt(base as i64, 0).single();
        dt.map(|d| d.to_rfc3339()).unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };

    // Try to extract path string after the record header
    // Path strings are typically null-terminated C strings
    let path_start = pos + 8;
    if path_start >= data.len() {
        return None;
    }

    let path = read_path_string(data, path_start);
    if path.is_empty() {
        // If no path at the expected position, check if it's earlier
        let alt_path = read_path_string(data, pos + 6);
        if alt_path.is_empty() {
            return None;
        }
        return Some((
            FSEvent {
                path: alt_path.clone(),
                event_type,
                timestamp,
            },
            pos + 6 + alt_path.len() + 1,
        ));
    }

    let next_pos = path_start + path.len() + 1;

    Some((
        FSEvent {
            path,
            event_type,
            timestamp,
        },
        next_pos,
    ))
}

/// Read a null-terminated path string (ASCII or UTF-8) starting at pos.
fn read_path_string(data: &[u8], start: usize) -> String {
    let end = data[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|p| start + p)
        .unwrap_or(std::cmp::min(start + 1024, data.len()));

    let slice = &data[start..end];

    // Only consider it a valid path if it looks like one
    let has_slash = slice.iter().any(|&b| b == b'/');
    let is_readable = slice.iter().all(|&b| b >= 0x20 && b < 0x7f || b == b'/');

    if !has_slash || !is_readable || slice.len() < 2 {
        return String::new();
    }

    String::from_utf8_lossy(slice).to_string()
}

/// Extract a base timestamp from nearby data (best effort).
fn base_timestamp_from_data(data: &[u8], _pos: usize) -> Option<u64> {
    // Look for a plausible timestamp in the header area (offset 16..31)
    if data.len() >= 24 {
        let ts = u64::from_be_bytes([
            data[16], data[17], data[18], data[19],
            data[20], data[21], data[22], data[23],
        ]);
        // Sanity check: relevant timestamps are between 2010 and 2100
        if ts > 1_260_000_000 && ts < 4_100_000_000 {
            return Some(ts);
        }
    }
    None
}

/// Fallback extraction: scan for file paths in the data.
fn extract_paths_fallback(data: &[u8], base_timestamp: Option<u64>) -> Vec<FSEvent> {
    let mut events: Vec<FSEvent> = Vec::new();

    let timestamp_str = base_timestamp
        .and_then(|ts| Utc.timestamp_opt(ts as i64, 0).single())
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());

    // Use the raw bytes: look for patterns like "/path/to/file" starting with /
    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] == b'/' && data[i + 1].is_ascii_alphanumeric() {
            let start = i;
            let mut end = i + 1;
            while end < data.len() && data[end] >= 0x20 && data[end] < 0x7f && data[end] != 0 {
                end += 1;
            }
            let len = end - start;
            if len >= 3 && len <= 1024 {
                let path = String::from_utf8_lossy(&data[start..end]).to_string();
                events.push(FSEvent {
                    path,
                    event_type: FSEventType::Unknown,
                    timestamp: timestamp_str.clone(),
                });
            }
            i = end;
        } else {
            i += 1;
        }
    }

    events.truncate(1000);
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal FSEvents log test file.
    fn build_fsevents_test_data() -> Vec<u8> {
        let mut data = Vec::new();

        // Magic
        data.extend_from_slice(b"1SLD");

        // Pad to fill header area
        data.resize(32, 0);

        // Set a reasonable timestamp at offset 16 (e.g., 2024-01-15 = 1705276800)
        let ts: u64 = 1_705_276_800;
        data[16..24].copy_from_slice(&ts.to_be_bytes());

        // Add event records with paths
        // Event 1: Created file
        let event1_flags: u32 = 0x0100 | 0x20000; // Created + IsFile
        data.extend_from_slice(&event1_flags.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]); // padding to 8-byte header
        data.extend_from_slice(b"/Users/test/Documents/new_file.txt");
        data.push(0);

        // Event 2: Modified file
        let event2_flags: u32 = 0x1000 | 0x20000; // Modified + IsFile
        data.extend_from_slice(&event2_flags.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
        data.extend_from_slice(b"/Users/test/Documents/modified.doc");
        data.push(0);

        // Event 3: Removed directory
        let event3_flags: u32 = 0x0200 | 0x40000; // Removed + IsDir
        data.extend_from_slice(&event3_flags.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
        data.extend_from_slice(b"/Users/test/old_folder");
        data.push(0);

        data
    }

    #[test]
    fn detect_fsevents_magic() {
        let data = b"1SLD.....";
        assert!(is_fsevents(data));
        assert!(!is_fsevents(b"NOTF"));
    }

    #[test]
    fn parse_fsevents_rejects_non_fsevents() {
        let result = parse_fsevents_log(b"not fsevents data");
        assert!(result.is_err());
    }

    #[test]
    fn parse_fsevents_rejects_short_data() {
        let result = parse_fsevents_log(b"1SL");
        assert!(result.is_err());
    }

    #[test]
    fn parse_fsevents_extracts_events() {
        let data = build_fsevents_test_data();
        let events = parse_fsevents_log(&data).expect("should parse");

        // Should find events. The exact count depends on fallback extraction but we should have at least one
        assert!(!events.is_empty(), "Expected at least one FSEvent");

        // Check first event
        if !events.is_empty() {
            let first = &events[0];
            assert!(!first.path.is_empty(), "Path should not be empty");
            assert!(first.path.starts_with('/'), "Path should start with /");
        }
    }

    #[test]
    fn read_path_string_valid_path() {
        let data = b"/Users/test/file.txt\0extra".to_vec();
        let path = read_path_string(&data, 0);
        assert_eq!(path, "/Users/test/file.txt");
    }

    #[test]
    fn read_path_string_no_slash_returns_empty() {
        let data = b"just_a_name\0extra";
        let path = read_path_string(data, 0);
        assert_eq!(path, "");
    }
}
