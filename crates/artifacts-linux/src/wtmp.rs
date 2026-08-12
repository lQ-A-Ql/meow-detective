//! /var/log/wtmp binary format parser.
//!
//! The wtmp file records logins, logouts, and system events in a binary format.
//! It shares the utmp(5) structure layout. On 32-bit systems the struct is
//! typically 384 bytes; on 64-bit glibc systems it is 400 bytes.
//!
//! This parser auto-detects the struct size by validating sampled records
//! against each candidate layout (utmp has no magic bytes).

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

/// /var/log/wtmp record types (subset of ut_type).
const EMPTY: i32 = 0;
const RUN_LVL: i32 = 1;
const BOOT_TIME: i32 = 2;
const _NEW_TIME: i32 = 3;
const _OLD_TIME: i32 = 4;
// Format constants — documents on-disk utmp record types.
const _INIT_PROCESS: i32 = 5;
const _LOGIN_PROCESS: i32 = 6;
const USER_PROCESS: i32 = 7;
const DEAD_PROCESS: i32 = 8;
const _ACCOUNTING: i32 = 9;

/// Known struct sizes for different architectures.
const WTMP_SIZE_32: usize = 384;
const WTMP_SIZE_64: usize = 400;
// 32-bit musl (e.g. i386 Alpine): musl uses a 64-bit time_t on every
// architecture (see musl `include/utmp.h` and the musl time64 transition),
// while `long` stays 32-bit, so `ut_session`/`tv_usec` are 4 bytes and the
// record totals 388 bytes.
const WTMP_SIZE_MUSL: usize = 388;

/// Offsets within the utmp struct differ by architecture.
/// For glibc 64-bit (x86_64):
///   ut_type: offset 0, 4 bytes (i32)
///   ut_pid:  offset 4, 4 bytes (i32) [actually pid_t which is i32 on Linux]
///   ut_line: offset 8, 32 bytes
///   ut_id:   offset 40, 4 bytes
///   ut_user: offset 44, 32 bytes
///   ut_host: offset 76, 256 bytes
///   ut_exit: offset 332, 4+4 bytes (struct exit_status)
///   ut_session: offset 340, 4 bytes (actually 8 on 64-bit? no - it's i32)
///   ut_tv:   offset 344, 8+8 bytes (timeval: tv_sec + tv_usec as 64-bit each on 64-bit Linux)
///   ut_addr_v6: offset 360, 16 bytes
///   __unused: offset 376, 20 bytes
/// Total: 396 bytes (but libc rounds to 400)
///
/// For glibc 32-bit (i386/i686):
///   ut_type: offset 0
///   ut_pid:  offset 4
///   ut_line: offset 8, 32 bytes
///   ut_id:   offset 40, 4 bytes
///   ut_user: offset 44, 32 bytes
///   ut_host: offset 76, 256 bytes
///   ut_exit: offset 332, 2+2 bytes
///   ut_session: offset 336, 4 bytes
///   ut_tv:   offset 340, 4+4 bytes (timeval: tv_sec + tv_usec as 32-bit)
///   ut_addr_v6: offset 348, 16 bytes
///   __unused: offset 364, 20 bytes
/// Total: 384 bytes
///
/// For 32-bit musl (e.g. i386 Alpine), per musl `include/utmp.h` with
/// 64-bit time_t and 32-bit long:
///   ut_type/pad: offset 0, 4 bytes; ut_pid: offset 4
///   ut_line: offset 8, 32 bytes; ut_id: offset 40, 4 bytes
///   ut_user: offset 44, 32 bytes; ut_host: offset 76, 256 bytes
///   ut_exit: offset 332, 4 bytes; ut_session: offset 336, 4 bytes
///   ut_tv: offset 340, 8+4 bytes (64-bit tv_sec, 32-bit tv_usec)
///   ut_addr_v6: offset 352, 16 bytes; __unused: offset 368, 20 bytes
/// Total: 388 bytes
///
/// A login record extracted from wtmp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRecord {
    /// Username
    pub user: String,
    /// Terminal line (e.g. "tty1", "pts/0")
    pub terminal: String,
    /// Remote host (empty for local logins)
    pub host: String,
    /// Login time (when USER_PROCESS record was found)
    pub login_time: Option<DateTime<Utc>>,
    /// Logout time (when DEAD_PROCESS was found for the same pid)
    pub logout_time: Option<DateTime<Utc>>,
    /// Process ID of the login session leader
    pub pid: i32,
    /// Record type (ut_type value)
    pub record_type: i32,
}

/// Internal parsed utmp record.
struct UtmpRecord {
    ut_type: i32,
    ut_pid: i32,
    ut_user: String,
    ut_line: String,
    ut_host: String,
    ut_tv_sec: i64,
    ut_tv_usec: i64,
}

/// Struct layout for a given word size.
#[derive(Clone)]
struct Layout {
    record_size: usize,
    off_type: usize,
    off_pid: usize,
    off_line: usize,
    len_line: usize,
    off_user: usize,
    len_user: usize,
    off_host: usize,
    len_host: usize,
    off_tv_sec: usize,
    /// Field width of `tv_sec` in bytes (4 on 32-bit glibc, 8 elsewhere).
    tv_sec_width: usize,
    off_tv_usec: usize,
    /// Field width of `tv_usec` in bytes (suseconds_t == long).
    tv_usec_width: usize,
}

fn detect_layout(data: &[u8]) -> Result<Layout, crate::LinuxArtifactError> {
    let candidates = [layout_64(), layout_32(), layout_musl()];

    // utmp has no magic bytes, so a bare length match is not enough: each
    // candidate must also pass content validation on its leading records.
    // A truncated trailing record (len % size != 0) is tolerated.
    for layout in &candidates {
        if data.len() >= layout.record_size && content_matches_layout(data, layout) {
            return Ok(layout.clone());
        }
    }

    Err(crate::LinuxArtifactError::ParseError {
        parser: "wtmp",
        message: "Cannot determine wtmp struct layout: no candidate passed content validation"
            .to_string(),
    })
}

/// Sample the leading complete records and require that at least half of
/// them (minimum one) look like plausible utmp entries.
fn content_matches_layout(data: &[u8], layout: &Layout) -> bool {
    let sampled = (data.len() / layout.record_size).min(8);
    let mut plausible = 0usize;
    for index in 0..sampled {
        let start = index * layout.record_size;
        let Some(record) = read_utmp_record(&data[start..start + layout.record_size], layout)
        else {
            continue;
        };
        if record_is_plausible(&record) {
            plausible += 1;
        }
    }
    plausible >= 1 && plausible * 2 >= sampled
}

fn record_is_plausible(record: &UtmpRecord) -> bool {
    if !(EMPTY..=_ACCOUNTING).contains(&record.ut_type) {
        return false;
    }
    if !record.ut_user.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return false;
    }
    // Zero timestamps occur on EMPTY/padding records; otherwise the login
    // time must sit within a sane 1990..2100 window.
    record.ut_tv_sec == 0 || (631_152_000..=4_102_444_800).contains(&record.ut_tv_sec)
}

fn layout_64() -> Layout {
    Layout {
        record_size: WTMP_SIZE_64,
        off_type: 0,
        off_pid: 4,
        off_line: 8,
        len_line: 32,
        off_user: 44,
        len_user: 32,
        off_host: 76,
        len_host: 256,
        off_tv_sec: 344,
        tv_sec_width: 8,
        off_tv_usec: 352,
        tv_usec_width: 8,
    }
}

fn layout_32() -> Layout {
    Layout {
        record_size: WTMP_SIZE_32,
        off_type: 0,
        off_pid: 4,
        off_line: 8,
        len_line: 32,
        off_user: 44,
        len_user: 32,
        off_host: 76,
        len_host: 256,
        off_tv_sec: 340,
        tv_sec_width: 4,
        off_tv_usec: 344,
        tv_usec_width: 4,
    }
}

fn layout_musl() -> Layout {
    // 32-bit musl: 64-bit time_t (musl is time64 on all architectures) but
    // 32-bit long for suseconds_t; see the module-level layout comment.
    Layout {
        record_size: WTMP_SIZE_MUSL,
        off_type: 0,
        off_pid: 4,
        off_line: 8,
        len_line: 32,
        off_user: 44,
        len_user: 32,
        off_host: 76,
        len_host: 256,
        off_tv_sec: 340,
        tv_sec_width: 8,
        off_tv_usec: 348,
        tv_usec_width: 4,
    }
}

fn null_terminated_string(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

fn read_utmp_record(data: &[u8], layout: &Layout) -> Option<UtmpRecord> {
    if data.len() < layout.record_size {
        return None;
    }

    let read_i32 = |off: usize| -> Option<i32> {
        let bytes = data.get(off..off + 4)?;
        Some(i32::from_le_bytes(bytes.try_into().ok()?))
    };

    let read_i64 = |off: usize| -> Option<i64> {
        let bytes = data.get(off..off + 8)?;
        Some(i64::from_le_bytes(bytes.try_into().ok()?))
    };

    // tv_sec / tv_usec width differs per layout: 32-bit glibc uses 4-byte
    // fields, 64-bit glibc 8-byte, 32-bit musl a mixed 8/4 pair.
    let read_time = |off: usize, width: usize| -> Option<i64> {
        if width == 4 {
            read_i32(off).map(i64::from)
        } else {
            read_i64(off)
        }
    };

    let ut_type = read_i32(layout.off_type)?;
    let ut_pid = read_i32(layout.off_pid)?;
    let ut_user =
        null_terminated_string(data.get(layout.off_user..layout.off_user + layout.len_user)?);
    let ut_line =
        null_terminated_string(data.get(layout.off_line..layout.off_line + layout.len_line)?);
    let ut_host =
        null_terminated_string(data.get(layout.off_host..layout.off_host + layout.len_host)?);
    let ut_tv_sec = read_time(layout.off_tv_sec, layout.tv_sec_width)?;
    let ut_tv_usec = read_time(layout.off_tv_usec, layout.tv_usec_width)?;

    Some(UtmpRecord {
        ut_type,
        ut_pid,
        ut_user,
        ut_line,
        ut_host,
        ut_tv_sec,
        ut_tv_usec,
    })
}

fn timestamp_from_utmp(sec: i64, _usec: i64) -> Option<DateTime<Utc>> {
    if sec <= 0 || sec > 4_102_444_800 {
        // year 2100
        return None;
    }
    Utc.timestamp_opt(sec, 0).single()
}

/// Decode the packed ut_pid of a RUN_LVL record: the low byte holds the
/// current runlevel as an ASCII character, the second byte the previous one.
fn runlevel_label(pid: i32) -> String {
    let current = (pid & 0xff) as u8;
    if current.is_ascii_graphic() {
        format!("runlevel-{}", current as char)
    } else {
        format!("runlevel-{pid}")
    }
}

/// Parse a wtmp binary file and extract login/logout records.
///
/// Returns a list of `LoginRecord` entries. Each USER_PROCESS record creates a login,
/// and matching DEAD_PROCESS records (by pid) set the logout time.
pub fn parse_wtmp(data: &[u8]) -> Result<Vec<LoginRecord>, crate::LinuxArtifactError> {
    if data.is_empty() {
        return Err(crate::LinuxArtifactError::ParseError {
            parser: "wtmp",
            message: "Empty wtmp data".to_string(),
        });
    }

    let layout = detect_layout(data)?;
    let num_records = data.len() / layout.record_size;

    let mut reader = Cursor::new(data);
    let mut records: Vec<LoginRecord> = Vec::new();
    let mut pending_logins: Vec<usize> = Vec::new(); // indices into records

    for _ in 0..num_records {
        let mut buf = vec![0u8; layout.record_size];
        if reader.read_exact(&mut buf).is_err() {
            break;
        }

        let ut = match read_utmp_record(&buf, &layout) {
            Some(r) => r,
            None => continue,
        };

        match ut.ut_type {
            USER_PROCESS => {
                let ts = timestamp_from_utmp(ut.ut_tv_sec, ut.ut_tv_usec);
                let record = LoginRecord {
                    user: ut.ut_user,
                    terminal: ut.ut_line,
                    host: ut.ut_host,
                    login_time: ts,
                    logout_time: None,
                    pid: ut.ut_pid,
                    record_type: ut.ut_type,
                };
                records.push(record);
                pending_logins.push(records.len() - 1);
            }
            DEAD_PROCESS => {
                let ts = timestamp_from_utmp(ut.ut_tv_sec, ut.ut_tv_usec);
                // Find the matching login record by pid
                if let Some(pos) = pending_logins
                    .iter()
                    .position(|&idx| idx < records.len() && records[idx].pid == ut.ut_pid)
                {
                    let idx = pending_logins.remove(pos);
                    if idx < records.len() {
                        records[idx].logout_time = ts;
                    }
                } else {
                    // DEAD_PROCESS without known USER_PROCESS - create a record
                    let record = LoginRecord {
                        user: ut.ut_user,
                        terminal: ut.ut_line,
                        host: ut.ut_host,
                        login_time: None,
                        logout_time: ts,
                        pid: ut.ut_pid,
                        record_type: ut.ut_type,
                    };
                    records.push(record);
                }
            }
            BOOT_TIME => {
                let ts = timestamp_from_utmp(ut.ut_tv_sec, ut.ut_tv_usec);
                let record = LoginRecord {
                    user: "reboot".to_string(),
                    terminal: "~".to_string(),
                    host: String::new(),
                    login_time: ts,
                    logout_time: None,
                    pid: 0,
                    record_type: ut.ut_type,
                };
                records.push(record);
            }
            RUN_LVL => {
                let ts = timestamp_from_utmp(ut.ut_tv_sec, ut.ut_tv_usec);
                let record = LoginRecord {
                    user: runlevel_label(ut.ut_pid),
                    terminal: "~".to_string(),
                    host: String::new(),
                    login_time: ts,
                    logout_time: None,
                    pid: 0,
                    record_type: ut.ut_type,
                };
                records.push(record);
            }
            _ => {
                // Other types (INIT_PROCESS, LOGIN_PROCESS, etc.)
                if ut.ut_user.is_empty() && ut.ut_type == EMPTY {
                    continue;
                }
                let ts = timestamp_from_utmp(ut.ut_tv_sec, ut.ut_tv_usec);
                let record = LoginRecord {
                    user: ut.ut_user,
                    terminal: ut.ut_line,
                    host: ut.ut_host,
                    login_time: ts,
                    logout_time: None,
                    pid: ut.ut_pid,
                    record_type: ut.ut_type,
                };
                records.push(record);
            }
        }
    }

    Ok(records)
}

#[cfg(test)]
#[path = "../tests/unit/wtmp.rs"]
mod tests;
