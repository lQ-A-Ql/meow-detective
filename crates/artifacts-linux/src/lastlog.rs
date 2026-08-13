//! /var/log/lastlog binary format parser.
//!
//! lastlog records each account's most recent login. The file is a sparse,
//! UID-indexed array of fixed-size `struct lastlog` records: the slot for UID
//! `n` sits at byte offset `n * sizeof(struct lastlog)`. Format sources:
//!
//! - glibc `login/lastlog.h`: `ll_time` is `int32_t` whenever
//!   `__WORDSIZE_TIME64_COMPAT32` holds (the default on x86_64 glibc, so the
//!   on-disk format keeps a 32-bit time even on 64-bit systems), followed by
//!   `ll_line[UT_LINESIZE]` (32) and `ll_host[UT_HOSTSIZE]` (256) — a 292-byte
//!   record: `ll_time @0`, `ll_line @4`, `ll_host @36`.
//! - glibc builds without the time64 compat shim use a 64-bit `ll_time`,
//!   producing a 296-byte record: `ll_time @0` (8 bytes), `ll_line @8`,
//!   `ll_host @40`.
//!
//! All multi-byte fields are native-endian; Linux disk images encountered in
//! practice are little-endian (x86, little-endian ARM), so this parser reads
//! little-endian. Slots for UIDs that never logged in are all-zero and are
//! skipped. A zero `ll_time` means "never logged in"; a slot that carries
//! line/host content despite a zero timestamp is kept with `time: None`.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// One account's most-recent login, recovered from a UID-indexed slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastlogRecord {
    /// UID owning this slot (the record's index within the file).
    pub uid: u32,
    /// Terminal line of the last login (e.g. "pts/0").
    pub line: String,
    /// Remote host of the last login (empty for local logins).
    pub host: String,
    /// Time of the last login; `None` when the slot has content but a zero
    /// or implausible timestamp.
    pub time: Option<DateTime<Utc>>,
}

/// Field offsets for one `struct lastlog` layout.
#[derive(Clone, Copy)]
struct Layout {
    record_size: usize,
    /// Width of `ll_time` in bytes (4 with the time64 compat shim, 8 without).
    time_width: usize,
    off_line: usize,
    off_host: usize,
}

/// glibc default (including x86_64): 32-bit `ll_time`.
const LAYOUT_TIME32: Layout = Layout {
    record_size: 292,
    time_width: 4,
    off_line: 4,
    off_host: 36,
};

/// glibc without `__WORDSIZE_TIME64_COMPAT32`: 64-bit `ll_time`.
const LAYOUT_TIME64: Layout = Layout {
    record_size: 296,
    time_width: 8,
    off_line: 8,
    off_host: 40,
};

const LINE_LEN: usize = 32;
const HOST_LEN: usize = 256;
/// Year 2100 in Unix seconds; timestamps beyond this are treated as corrupt.
const MAX_SANE_TIMESTAMP: i64 = 4_102_444_800;

/// Parse a lastlog binary file into per-UID last-login records.
///
/// An empty file yields an empty vector (no UID ever logged in). A truncated
/// trailing record is tolerated; the chosen layout must still pass content
/// validation on the complete records that precede it.
pub fn parse_lastlog(data: &[u8]) -> Result<Vec<LastlogRecord>, crate::LinuxArtifactError> {
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
        let raw_time = read_time(slot, &layout);
        records.push(LastlogRecord {
            uid,
            line: null_terminated_string(&slot[layout.off_line..layout.off_line + LINE_LEN]),
            host: null_terminated_string(&slot[layout.off_host..layout.off_host + HOST_LEN]),
            time: sane_timestamp(raw_time),
        });
    }
    Ok(records)
}

fn detect_layout(data: &[u8]) -> Result<Layout, crate::LinuxArtifactError> {
    // lastlog has no magic bytes; like wtmp, layout detection combines exact
    // divisibility with content plausibility of the sampled slots.
    let candidates = [LAYOUT_TIME32, LAYOUT_TIME64];
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
        parser: "lastlog",
        message: "Cannot determine lastlog record layout: no candidate passed content validation"
            .to_string(),
    })
}

/// Tally the leading complete slots (up to 8): all-zero slots are neutral,
/// non-empty slots count as plausible when the terminal line is printable
/// ASCII and the timestamp is zero or sane.
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
        let line = null_terminated_string(&slot[layout.off_line..layout.off_line + LINE_LEN]);
        let raw_time = read_time(slot, layout);
        let printable = line.bytes().all(|b| (0x20..=0x7e).contains(&b));
        let time_ok = raw_time == 0 || (0 < raw_time && raw_time <= MAX_SANE_TIMESTAMP);
        if printable && time_ok {
            plausible += 1;
        }
    }
    (non_empty, plausible)
}

fn read_time(slot: &[u8], layout: &Layout) -> i64 {
    if layout.time_width == 4 {
        i64::from(i32::from_le_bytes(slot[0..4].try_into().unwrap_or([0; 4])))
    } else {
        i64::from_le_bytes(slot[0..8].try_into().unwrap_or([0; 8]))
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
#[path = "../tests/unit/lastlog.rs"]
mod tests;
