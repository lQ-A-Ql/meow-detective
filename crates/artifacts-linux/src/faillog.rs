//! /var/log/faillog binary format parser.
//!
//! faillog tracks per-account login failure counters. Like lastlog it is a
//! sparse, UID-indexed array of fixed-size records (slot for UID `n` at
//! offset `n * sizeof(struct faillog)`). Format source: shadow-maint/shadow
//! `lib/faillog.h` (mirrored by faillog(5)):
//!
//! ```c
//! struct faillog {
//!     short   fail_cnt;      /* failures since last success */
//!     short   fail_max;      /* failures before turning account off */
//!     char    fail_line[12]; /* last failure occurred here */
//!     time_t  fail_time;     /* last failure occurred then */
//!     long    fail_locktime; /* secs account is locked after */
//! };
//! ```
//!
//! Unlike lastlog, faillog has no time-compat shim: `time_t` and `long`
//! follow the host word size, so the on-disk record is 24 bytes on 32-bit
//! (ILP32) systems — `fail_time @16` (i32), `fail_locktime @20` (i32) — and
//! 32 bytes on 64-bit (LP64) systems — `fail_time @16` (i64),
//! `fail_locktime @24` (i64). All fields are native-endian; Linux images in
//! practice are little-endian. All-zero slots (no failures recorded) are
//! skipped.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// One account's login-failure state, recovered from a UID-indexed slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaillogRecord {
    /// UID owning this slot (the record's index within the file).
    pub uid: u32,
    /// Consecutive failures since the last successful login (`fail_cnt`).
    pub failure_count: i16,
    /// Lockout threshold: failures before the account is turned off
    /// (`fail_max`; 0 disables the limit).
    pub max_failures: i16,
    /// Terminal line of the most recent failure (`fail_line`).
    pub line: String,
    /// Time of the most recent failure; `None` when zero or implausible.
    pub last_failure: Option<DateTime<Utc>>,
    /// Seconds the account stays locked after a failure (`fail_locktime`).
    pub locktime_seconds: i64,
    /// `fail_cnt` has reached a nonzero `fail_max` lockout threshold.
    pub lockout: bool,
}

/// Field offsets for one `struct faillog` layout.
#[derive(Clone, Copy)]
struct Layout {
    record_size: usize,
    /// Width of `fail_time`/`fail_locktime` in bytes (4 on ILP32, 8 on LP64).
    word_width: usize,
    off_locktime: usize,
}

/// 64-bit LP64: `time_t`/`long` are 8 bytes.
const LAYOUT_LP64: Layout = Layout {
    record_size: 32,
    word_width: 8,
    off_locktime: 24,
};

/// 32-bit ILP32: `time_t`/`long` are 4 bytes.
const LAYOUT_ILP32: Layout = Layout {
    record_size: 24,
    word_width: 4,
    off_locktime: 20,
};

const OFF_COUNT: usize = 0;
const OFF_MAX: usize = 2;
const OFF_LINE: usize = 4;
const LINE_LEN: usize = 12;
const OFF_TIME: usize = 16;
/// Year 2100 in Unix seconds; timestamps beyond this are treated as corrupt.
const MAX_SANE_TIMESTAMP: i64 = 4_102_444_800;

/// Parse a faillog binary file into per-UID failure records.
///
/// An empty file yields an empty vector. A truncated trailing record is
/// tolerated; the chosen layout must still pass content validation.
pub fn parse_faillog(data: &[u8]) -> Result<Vec<FaillogRecord>, crate::LinuxArtifactError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let layout = detect_layout(data)?;
    let num_records = data.len() / layout.record_size;
    let mut records = Vec::new();
    for index in 0..num_records {
        let start = index * layout.record_size;
        let Some(slot) = data.get(start..start + layout.record_size) else {
            break;
        };
        if slot.iter().all(|&byte| byte == 0) {
            continue;
        }
        // The read caps upstream keep index far below u32::MAX; guard anyway.
        let Ok(uid) = u32::try_from(index) else {
            break;
        };
        let failure_count = read_i16(slot, OFF_COUNT);
        let max_failures = read_i16(slot, OFF_MAX);
        let raw_time = read_word(slot, OFF_TIME, layout.word_width);
        let locktime_seconds = read_word(slot, layout.off_locktime, layout.word_width);
        records.push(FaillogRecord {
            uid,
            failure_count,
            max_failures,
            line: null_terminated_string(&slot[OFF_LINE..OFF_LINE + LINE_LEN]),
            last_failure: sane_timestamp(raw_time),
            locktime_seconds,
            lockout: max_failures > 0 && failure_count >= max_failures,
        });
    }
    Ok(records)
}

fn detect_layout(data: &[u8]) -> Result<Layout, crate::LinuxArtifactError> {
    // faillog has no magic bytes; detection combines exact divisibility with
    // content plausibility of the sampled slots, preferring LP64 (the common
    // case for modern images) among equally plausible exact divisors.
    let candidates = [LAYOUT_LP64, LAYOUT_ILP32];
    let mut best: Option<(Layout, usize)> = None;
    for layout in candidates
        .iter()
        .filter(|layout| data.len().is_multiple_of(layout.record_size))
    {
        let (non_empty, plausible) = content_score(data, layout);
        if plausible >= 1
            && plausible * 2 >= non_empty
            && best.is_none_or(|(_, score)| plausible > score)
        {
            best = Some((*layout, plausible));
        }
    }
    if let Some((layout, _)) = best {
        return Ok(layout);
    }

    // Fallback: tolerate a truncated trailing record by scoring non-dividing
    // candidates in declaration order.
    for layout in &candidates {
        if data.len() >= layout.record_size {
            let (non_empty, plausible) = content_score(data, layout);
            if plausible >= 1 && plausible * 2 >= non_empty {
                return Ok(*layout);
            }
        }
    }

    Err(crate::LinuxArtifactError::ParseError {
        parser: "faillog",
        message: "Cannot determine faillog record layout: no candidate passed content validation"
            .to_string(),
    })
}

/// Tally the leading complete slots (up to 8): a slot is plausible when the
/// counters are non-negative, the terminal line is printable ASCII, and the
/// timestamp is zero or sane.
fn content_score(data: &[u8], layout: &Layout) -> (usize, usize) {
    let sampled = (data.len() / layout.record_size).min(8);
    let mut non_empty = 0usize;
    let mut plausible = 0usize;
    for index in 0..sampled {
        let start = index * layout.record_size;
        let slot = &data[start..start + layout.record_size];
        if slot.iter().all(|&byte| byte == 0) {
            continue;
        }
        non_empty += 1;
        let line = null_terminated_string(&slot[OFF_LINE..OFF_LINE + LINE_LEN]);
        let raw_time = read_word(slot, OFF_TIME, layout.word_width);
        let printable = line.bytes().all(|b| (0x20..=0x7e).contains(&b));
        let counters_ok = read_i16(slot, OFF_COUNT) >= 0 && read_i16(slot, OFF_MAX) >= 0;
        let time_ok = raw_time == 0 || (0 < raw_time && raw_time <= MAX_SANE_TIMESTAMP);
        if printable && counters_ok && time_ok {
            plausible += 1;
        }
    }
    (non_empty, plausible)
}

fn read_i16(slot: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes(slot[offset..offset + 2].try_into().unwrap_or([0; 2]))
}

fn read_word(slot: &[u8], offset: usize, width: usize) -> i64 {
    if width == 4 {
        i64::from(i32::from_le_bytes(
            slot[offset..offset + 4].try_into().unwrap_or([0; 4]),
        ))
    } else {
        i64::from_le_bytes(slot[offset..offset + 8].try_into().unwrap_or([0; 8]))
    }
}

fn sane_timestamp(sec: i64) -> Option<DateTime<Utc>> {
    if sec <= 0 || sec > MAX_SANE_TIMESTAMP {
        return None;
    }
    Utc.timestamp_opt(sec, 0).single()
}

fn null_terminated_string(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

#[cfg(test)]
#[path = "../tests/unit/faillog.rs"]
mod tests;
