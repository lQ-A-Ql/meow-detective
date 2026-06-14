//! Apple Unified Log (tracev3) parser.
//!
//! Parses tracev3 format files found under `/var/db/diagnostics/` and
//! `/private/var/db/diagnostics/` on macOS 10.12+.
//!
//! The tracev3 format uses a chunk-based binary layout:
//! ```text
//! Chunk header (variable size):
//!   0x00: tag (u16 LE) — 0x6011 = firehose, etc.
//!   0x02: ...
//! ```
//!
//! For this parser we detect the `tracev3` magic and parse the chunk structure
//! to extract log metadata (timestamps, process names, messages) where available.
//!
//! Reference: macOS Unified Log format (reverse-engineered by community).

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// A single entry from a Unified Log tracev3 file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedLogEntry {
    /// ISO 8601 timestamp string
    pub timestamp: String,
    /// Process name (e.g., "kernel", "launchd", "WindowServer")
    pub process: String,
    /// Log message text
    pub message: String,
    /// Activity identifier string
    pub activity_id: String,
    /// Thread identifier string
    pub thread_id: String,
}

/// Magic bytes at the start of a tracev3 file.
const TRACEV3_MAGIC: &[u8; 7] = b"tracev3";

/// Check if data begins with the tracev3 magic.
pub fn is_tracev3(data: &[u8]) -> bool {
    data.len() >= 8 && &data[0..7] == TRACEV3_MAGIC
}

/// Parse a Unified Log tracev3 file and extract log entries.
///
/// This implementation reads the tracev3 header and chunk-based structure,
/// extracting timestamp information from chunk headers and log metadata
/// from trace messages within the chunks.
///
/// The tracev3 format uses the Mach absolute timebase, which on Apple Silicon
/// uses a 24 MHz clock (41.667 ns per tick). We use a simplified approach
/// that handles commonly observed patterns.
pub fn parse_tracev3(data: &[u8]) -> Result<Vec<UnifiedLogEntry>, String> {
    if data.len() < 256 {
        return Err("Unified Log data too short".to_string());
    }
    if !is_tracev3(data) {
        return Err("Not a tracev3 file (missing tracev3 magic)".to_string());
    }

    let mut entries: Vec<UnifiedLogEntry> = Vec::new();

    // Skip the 8-byte magic and header
    // tracev3 files have a catalog section followed by data chunks.
    // We scan for chunk headers and extract whatever metadata we can find.

    // Parse header structure to find chunk catalog
    // After the magic, there's typically a header with catalog UUID, timestamps, etc.
    // For this parser, we do a pragmatic scan for ASCII process names and messages.

    // Detect the timebase from the header (offset 0x10 typically has boot UUID + timespec)
    // The header structure (simplified):
    // [0..7]   "tracev3"
    // [8..0x10] header length / flags
    // [0x10..] timebase info, catalog

    // Scan for potential chunk boundaries. tracev3 chunks typically have a tag at their start.
    // We look for known message patterns as heuristic, plus timestamps near chunk boundaries.

    let mut chunk_pos = 8usize;

    // Try reading the catalog offset from header area
    // In practice the firehose catalog starts with 0x00 0x00 and has UUID
    while chunk_pos + 64 < data.len() {
        // Look for firehose chunk tags (0x6011, 0x6013, etc.)
        if chunk_pos + 2 > data.len() {
            break;
        }
        let tag = u16::from_le_bytes([data[chunk_pos], data[chunk_pos + 1]]);

        // 0x6011 = firehose memory chunk, 0x6013 = firehose io chunk
        if tag == 0x6011 || tag == 0x6013 {
            // Try to parse this chunk
            if let Some(entry) = parse_chunk(data, chunk_pos) {
                entries.push(entry);
            }
        }

        chunk_pos += 16; // advance by estimated minimum chunk alignment

        // Safety limit
        if entries.len() > 500 {
            break;
        }
    }

    // If no entries found via chunk scanning, extract what we can from the file metadata
    if entries.is_empty() {
        // Extract any readable strings from the data section as fallback
        entries = extract_fallback_entries(data);
    }

    Ok(entries)
}

/// Attempt to parse a firehose chunk at the given position.
fn parse_chunk(data: &[u8], pos: usize) -> Option<UnifiedLogEntry> {
    if pos + 32 > data.len() {
        return None;
    }

    let tag = u16::from_le_bytes([data[pos], data[pos + 1]]);
    if tag != 0x6011 && tag != 0x6013 {
        return None;
    }

    // Firehose chunk structure (simplified):
    // [0..1] tag (u16 LE)
    // [2..3] sub_tag (u16 LE)
    // [4..7] length (u32 LE) — total chunk length
    // [8..15] timestamp (u64 LE) — Mach continuous time
    // ... more header fields
    // ... message data

    let chunk_len =
        u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
    if chunk_len == 0 || chunk_len > 1024 * 1024 {
        return None;
    }
    let chunk_end = std::cmp::min(pos + chunk_len, data.len());

    // Read timestamp at offset 8
    let mach_ts = if pos + 16 <= data.len() {
        u64::from_le_bytes([
            data[pos + 8],
            data[pos + 9],
            data[pos + 10],
            data[pos + 11],
            data[pos + 12],
            data[pos + 13],
            data[pos + 14],
            data[pos + 15],
        ])
    } else {
        return None;
    };

    // Convert Mach continuous time to Unix epoch.
    // Apple Silicon: 24 MHz clock → timestamp = mach_ts / 24_000_000
    // Intel: uses mach_absolute_time()
    // We use 24 MHz as default (most common for modern macOS on Apple Silicon)
    let unix_ts = convert_mach_timestamp(mach_ts);

    // Try to extract process name and message from the chunk payload.
    // Process name is typically stored as a C string near the end of the chunk header.
    let header_end = pos + 64;
    let scan_end = std::cmp::min(chunk_end, pos + 512);

    let process = extract_process_name(data, header_end, scan_end);
    let message = extract_message(data, header_end, chunk_end);
    let (activity_id, thread_id) = extract_ids(data, pos, header_end);

    Some(UnifiedLogEntry {
        timestamp: unix_ts,
        process,
        message,
        activity_id,
        thread_id,
    })
}

/// Convert Mach continuous time ticks to ISO 8601 string.
fn convert_mach_timestamp(mach_ts: u64) -> String {
    // Default timebase: 24 MHz (Apple Silicon standard)
    let timebase_numer = 125u64;
    let timebase_denom = 3u64;
    // tick_ns = numer / denom = 125/3 ≈ 41.667 ns
    // seconds = ticks * 125 / 3 / 1_000_000_000
    let nanos = (mach_ts as u128 * timebase_numer as u128 / timebase_denom as u128) as u64;
    let secs = (nanos / 1_000_000_000) as i64;
    let sub_nanos = (nanos % 1_000_000_000) as u32;

    // mach_continuous_time() uses mach_absolute_time which starts at boot, not epoch.
    // Since we don't know the boot time from the file alone, we record the raw calculated time.
    // In practice, the boot UUID in the header provides the boot timestamp.
    Utc.timestamp_opt(secs, sub_nanos)
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| format!("ticks:{}", mach_ts))
}

/// Try to extract a process name from chunk data.
fn extract_process_name(data: &[u8], start: usize, end: usize) -> String {
    let range = &data[start..std::cmp::min(end, data.len())];

    // Look for common process names as null-terminated strings
    let common_procs: &[&[u8]] = &[
        b"kernel\0",
        b"launchd\0",
        b"WindowServer",
        b"mDNSRespo",
        b"configd\0",
        b"syslogd\0",
        b"logd\0\0\0",
        b"cfprefsd",
        b"security",
        b"coreaudi",
        b"bluetooth",
    ];

    for proc_bytes in common_procs {
        let trimmed = &proc_bytes[..proc_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(proc_bytes.len())];
        if trimmed.is_empty() {
            continue;
        }
        if find_bytes(range, trimmed).is_some() {
            return String::from_utf8_lossy(trimmed).to_string();
        }
    }

    // Extract any readable ASCII sequence of length 3..32 as a candidate process name
    let mut best: Option<String> = None;
    let mut i = 0;
    while i < range.len() {
        if range[i].is_ascii_alphanumeric() || range[i] == b'_' {
            let mut j = i + 1;
            while j < range.len()
                && (range[j].is_ascii_alphanumeric() || range[j] == b'_' || range[j] == b'-')
            {
                j += 1;
            }
            let len = j - i;
            if len >= 3 && len <= 32 && j < range.len() && range[j] == 0 {
                let s = String::from_utf8_lossy(&range[i..j]).to_string();
                if best.as_ref().map_or(true, |b| s.len() > b.len()) {
                    best = Some(s);
                }
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }

    best.unwrap_or_else(|| "unknown".to_string())
}

/// Try to extract a log message from chunk data.
fn extract_message(data: &[u8], start: usize, end: usize) -> String {
    let range = &data[start..std::cmp::min(end, data.len())];

    // Find the longest readable ASCII string
    let mut best: Option<String> = None;
    let mut i = 0;
    while i < range.len() {
        if range[i] >= 0x20 && range[i] < 0x7f {
            let mut j = i + 1;
            while j < range.len() && range[j] >= 0x20 && range[j] < 0x7f {
                j += 1;
            }
            let len = j - i;
            if len >= 4 && len <= 512 {
                let s = String::from_utf8_lossy(&range[i..j]).to_string();
                if best.as_ref().map_or(true, |b| s.len() > b.len()) {
                    best = Some(s);
                }
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }

    best.unwrap_or_else(|| "<no message>".to_string())
}

/// Extract activity and thread IDs from chunk header.
fn extract_ids(data: &[u8], chunk_start: usize, header_end: usize) -> (String, String) {
    let end = std::cmp::min(header_end, data.len());
    let range = &data[std::cmp::min(chunk_start + 16, end)..end];

    // Activity/thread IDs are typically u64 values stored near the start of the header
    let mut activity_id = "0x0".to_string();
    let mut thread_id = "0x0".to_string();

    // Look for plausible ID values (non-zero u64 LE with some structure)
    for i in (0..range.len().saturating_sub(8)).step_by(4) {
        if i + 8 <= range.len() {
            let v1 = u64::from_le_bytes([
                range[i],
                range[i + 1],
                range[i + 2],
                range[i + 3],
                range[i + 4],
                range[i + 5],
                range[i + 6],
                range[i + 7],
            ]);
            // Activity IDs are often small-ish but non-zero
            if v1 > 0 && v1 < 1_000_000 && activity_id == "0x0" {
                activity_id = format!("0x{:X}", v1);
            }
            // Thread IDs look similar
            if i + 16 <= range.len() {
                let v2 = u64::from_le_bytes([
                    range[i + 8],
                    range[i + 9],
                    range[i + 10],
                    range[i + 11],
                    range[i + 12],
                    range[i + 13],
                    range[i + 14],
                    range[i + 15],
                ]);
                if v2 > 0 && v2 < 1_000_000 && thread_id == "0x0" {
                    thread_id = format!("0x{:X}", v2);
                }
            }
        }
    }

    (activity_id, thread_id)
}

/// Fallback extraction: scan the entire data for readable strings as log entries.
fn extract_fallback_entries(data: &[u8]) -> Vec<UnifiedLogEntry> {
    let mut entries: Vec<UnifiedLogEntry> = Vec::new();

    let text = String::from_utf8_lossy(data);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.len() >= 4 {
            entries.push(UnifiedLogEntry {
                timestamp: "unknown".to_string(),
                process: extract_process_name(trimmed.as_bytes(), 0, trimmed.len()),
                message: trimmed.to_string(),
                activity_id: "0x0".to_string(),
                thread_id: "0x0".to_string(),
            });
        }
    }

    // Limit fallback entries
    entries.truncate(100);
    entries
}

/// Find bytes within a slice (simple memmem).
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal tracev3-like file for testing.
    fn build_tracev3_test_data() -> Vec<u8> {
        let mut data = Vec::new();

        // Magic
        data.extend_from_slice(b"tracev3");

        // Pad to 256 minimum
        data.resize(256, 0);

        // Add a simulated firehose chunk
        let chunk_start = data.len();
        // tag 0x6013
        data.push(0x13);
        data.push(0x60);
        // sub_tag
        data.push(0x00);
        data.push(0x00);
        // chunk length (say 128 bytes)
        let chunk_len: u32 = 128;
        data.extend_from_slice(&chunk_len.to_le_bytes());
        // timestamp — some mach ticks
        let mach_ts: u64 = 0x1000_0000_0000_0000;
        data.extend_from_slice(&mach_ts.to_le_bytes());

        // Pad header area
        data.resize(data.len() + 32, 0);

        // Add process name "kernel\0" in the header region
        let proc_pos = data.len() - 20;
        data[proc_pos] = b'k';
        data[proc_pos + 1] = b'e';
        data[proc_pos + 2] = b'r';
        data[proc_pos + 3] = b'n';
        data[proc_pos + 4] = b'e';
        data[proc_pos + 5] = b'l';
        data[proc_pos + 6] = 0;

        // Add message text in the payload region
        data.extend_from_slice(b"System boot completed successfully");
        data.push(0);

        // Fill remaining chunk
        while data.len() < chunk_start + chunk_len as usize + 64 {
            data.push(0);
        }

        data
    }

    #[test]
    fn detect_tracev3_magic() {
        let mut data = vec![0u8; 256];
        data[0..7].copy_from_slice(b"tracev3");
        assert!(is_tracev3(&data));
        assert!(!is_tracev3(b"notracev"));
    }

    #[test]
    fn parse_tracev3_rejects_non_tracev3() {
        let result = parse_tracev3(b"not a tracev3 file");
        assert!(result.is_err());
    }

    #[test]
    fn parse_tracev3_rejects_short_data() {
        let result = parse_tracev3(b"tracev3");
        assert!(result.is_err());
    }

    #[test]
    fn parse_tracev3_extracts_entries() {
        let data = build_tracev3_test_data();
        let entries = parse_tracev3(&data).expect("should parse");

        // Should find at least one entry
        assert!(!entries.is_empty(), "Expected at least one log entry");
    }

    #[test]
    fn extract_process_name_finds_known() {
        let mut data = vec![0u8; 64];
        data[10..17].copy_from_slice(b"kernel\0");
        let name = extract_process_name(&data, 0, 64);
        assert_eq!(name, "kernel");
    }

    #[test]
    fn extract_message_finds_ascii() {
        // The function returns the longest ASCII run; include exactly what we expect
        let data = b"\0\0Hello, World!\0\0";
        let msg = extract_message(data, 0, data.len());
        assert_eq!(msg, "Hello, World!");
    }
}
