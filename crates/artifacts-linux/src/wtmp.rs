//! /var/log/wtmp binary format parser.
//!
//! The wtmp file records logins, logouts, and system events in a binary format.
//! It shares the utmp(5) structure layout. On 32-bit systems the struct is
//! typically 384 bytes; on 64-bit glibc systems it is 400 bytes.
//!
//! This parser auto-detects the struct size by checking file length divisibility.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

/// /var/log/wtmp record types (subset of ut_type).
const EMPTY: i32 = 0;
const RUN_LVL: i32 = 1;
const BOOT_TIME: i32 = 2;
const _NEW_TIME: i32 = 3;
const _OLD_TIME: i32 = 4;
#[allow(dead_code)]
const INIT_PROCESS: i32 = 5;
#[allow(dead_code)]
const LOGIN_PROCESS: i32 = 6;
const USER_PROCESS: i32 = 7;
const DEAD_PROCESS: i32 = 8;
const _ACCOUNTING: i32 = 9;

/// Known struct sizes for different architectures.
const WTMP_SIZE_32: usize = 384;
const WTMP_SIZE_64: usize = 400;
// Some musl-based systems use a compact 340-byte struct
const WTMP_SIZE_MUSL: usize = 340;

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
    off_tv_usec: usize,
}

fn detect_layout(data: &[u8]) -> Result<Layout, String> {
    let candidates: [(usize, &dyn Fn() -> Layout); 3] = [
        (WTMP_SIZE_64, &|| layout_64()),
        (WTMP_SIZE_32, &|| layout_32()),
        (WTMP_SIZE_MUSL, &|| layout_musl()),
    ];

    for (size, layout_fn) in &candidates {
        if data.len().is_multiple_of(*size) && data.len() >= *size {
            return Ok(layout_fn());
        }
    }

    // If none match exactly, try modulo on the largest size
    for (size, layout_fn) in &candidates {
        if data.len() >= *size {
            return Ok(layout_fn());
        }
    }

    Err("Cannot determine wtmp struct layout: file too small".to_string())
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
        off_tv_usec: 352,
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
        off_tv_usec: 344,
    }
}

fn layout_musl() -> Layout {
    Layout {
        record_size: WTMP_SIZE_MUSL,
        off_type: 0,
        off_pid: 4,
        off_line: 8,
        len_line: 32,
        off_user: 40,
        len_user: 32,
        off_host: 72,
        len_host: 256,
        off_tv_sec: 328,
        off_tv_usec: 332,
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

    let ut_type = read_i32(layout.off_type)?;
    let ut_pid = read_i32(layout.off_pid)?;
    let ut_user =
        null_terminated_string(data.get(layout.off_user..layout.off_user + layout.len_user)?);
    let ut_line =
        null_terminated_string(data.get(layout.off_line..layout.off_line + layout.len_line)?);
    let ut_host =
        null_terminated_string(data.get(layout.off_host..layout.off_host + layout.len_host)?);
    let ut_tv_sec = read_i64(layout.off_tv_sec)?;
    let ut_tv_usec = read_i64(layout.off_tv_usec)?;

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

/// Parse a wtmp binary file and extract login/logout records.
///
/// Returns a list of `LoginRecord` entries. Each USER_PROCESS record creates a login,
/// and matching DEAD_PROCESS records (by pid) set the logout time.
pub fn parse_wtmp(data: &[u8]) -> Result<Vec<LoginRecord>, String> {
    if data.is_empty() {
        return Err("Empty wtmp data".to_string());
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
                    user: format!("runlevel-{}", ut.ut_pid),
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
mod tests {
    use super::*;

    fn build_wtmp_record_64(
        ut_type: i32,
        ut_pid: i32,
        user: &str,
        line: &str,
        host: &str,
        tv_sec: i64,
        tv_usec: i64,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; WTMP_SIZE_64];
        buf[0..4].copy_from_slice(&ut_type.to_le_bytes());
        buf[4..8].copy_from_slice(&ut_pid.to_le_bytes());

        // ut_line at offset 8
        let line_bytes = line.as_bytes();
        let copy_len = line_bytes.len().min(32);
        buf[8..8 + copy_len].copy_from_slice(&line_bytes[..copy_len]);

        // ut_user at offset 44
        let user_bytes = user.as_bytes();
        let copy_len = user_bytes.len().min(32);
        buf[44..44 + copy_len].copy_from_slice(&user_bytes[..copy_len]);

        // ut_host at offset 76
        let host_bytes = host.as_bytes();
        let copy_len = host_bytes.len().min(256);
        buf[76..76 + copy_len].copy_from_slice(&host_bytes[..copy_len]);

        // ut_tv at offset 344
        buf[344..352].copy_from_slice(&tv_sec.to_le_bytes());
        buf[352..360].copy_from_slice(&tv_usec.to_le_bytes());

        buf
    }

    #[test]
    fn parse_wtmp_64_login_logout() {
        let login_ts: i64 = 1_700_000_000;
        let logout_ts: i64 = 1_700_010_000;

        let mut data = Vec::new();
        // USER_PROCESS record for "alice" on pts/0
        data.extend(build_wtmp_record_64(
            USER_PROCESS,
            12345,
            "alice",
            "pts/0",
            "192.168.1.100",
            login_ts,
            0,
        ));
        // DEAD_PROCESS for pid 12345
        data.extend(build_wtmp_record_64(
            DEAD_PROCESS,
            12345,
            "",
            "pts/0",
            "",
            logout_ts,
            0,
        ));

        let records = parse_wtmp(&data).expect("should parse wtmp");
        assert_eq!(records.len(), 1);

        // The USER_PROCESS record has logout_time set by matching DEAD_PROCESS
        assert_eq!(records[0].user, "alice");
        assert_eq!(records[0].terminal, "pts/0");
        assert_eq!(records[0].host, "192.168.1.100");
        assert_eq!(records[0].pid, 12345);
        assert!(records[0].login_time.is_some());
        assert!(records[0].logout_time.is_some());
        assert_eq!(records[0].login_time.unwrap().timestamp(), login_ts);
        assert_eq!(records[0].logout_time.unwrap().timestamp(), logout_ts);
    }

    #[test]
    fn parse_wtmp_boot_record() {
        let boot_ts: i64 = 1_700_000_000;
        let data = build_wtmp_record_64(BOOT_TIME, 0, "reboot", "~", "", boot_ts, 0);

        let records = parse_wtmp(&data).expect("should parse wtmp");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].user, "reboot");
        assert_eq!(records[0].record_type, BOOT_TIME);
    }

    #[test]
    fn parse_wtmp_32bit_layout() {
        // Build a 32-bit layout record
        let mut buf = vec![0u8; WTMP_SIZE_32];
        buf[0..4].copy_from_slice(&USER_PROCESS.to_le_bytes());
        buf[4..8].copy_from_slice(&9999i32.to_le_bytes());
        buf[44..48].copy_from_slice(b"bob\0");
        buf[8..12].copy_from_slice(b"tty2");
        // timeval at offset 340 (sec=32bit, usec=32bit)
        let login_ts: i64 = 1_700_000_000;
        buf[340..344].copy_from_slice(&(login_ts as i32).to_le_bytes());
        buf[344..348].copy_from_slice(&0i32.to_le_bytes());

        let records = parse_wtmp(&buf).expect("should parse 32-bit wtmp");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].user, "bob");
    }

    #[test]
    fn reject_empty_data() {
        let result = parse_wtmp(&[]);
        assert!(result.is_err());
    }
}
