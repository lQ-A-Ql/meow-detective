use super::*;
use crate::analysis_service::extraction::reader::CandidateSource;
use chrono::NaiveDate;
use std::sync::atomic::AtomicBool;

fn naive(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d)
        .and_then(|date| date.and_hms_opt(hh, mm, ss))
        .expect("valid naive datetime")
}

fn conn_with_files(paths: &[&str]) -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute(
        "CREATE TABLE file_entries (
            id TEXT PRIMARY KEY,
            parent_id TEXT,
            data_source_id TEXT NOT NULL,
            path TEXT NOT NULL,
            name TEXT NOT NULL,
            entry_type TEXT NOT NULL,
            size INTEGER,
            ext TEXT,
            deleted INTEGER NOT NULL DEFAULT 0,
            hidden INTEGER NOT NULL DEFAULT 0,
            system INTEGER NOT NULL DEFAULT 0,
            created_at TEXT,
            modified_at TEXT,
            accessed_at TEXT,
            changed_at TEXT,
            hash_sha256 TEXT,
            encrypted INTEGER
        )",
        [],
    )
    .expect("create file_entries");
    for (index, path) in paths.iter().enumerate() {
        let name = path.rsplit('/').next().unwrap_or(path);
        conn.execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, encrypted)
             VALUES (?1, 'ds-tz', ?2, ?3, 'file', 128, 0)",
            rusqlite::params![format!("tz-file-{index}"), path, name],
        )
        .expect("insert file entry");
    }
    conn
}

fn bytes_reader(
    content: &'static str,
) -> impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String> {
    move |_, _| Ok(CandidateSource::Bytes(content.as_bytes().to_vec()))
}

#[test]
fn localtime_symlink_target_yields_zone_name() {
    assert_eq!(
        zone_from_localtime("../usr/share/zoneinfo/Asia/Shanghai\n").as_deref(),
        Some("Asia/Shanghai")
    );
    assert_eq!(
        zone_from_localtime("/usr/share/zoneinfo/Europe/Berlin").as_deref(),
        Some("Europe/Berlin")
    );
    // A copied TZif binary (not a symlink) yields no zone name.
    assert_eq!(zone_from_localtime("TZif2\0\0\0binary"), None);
}

#[test]
fn timezone_file_yields_zone_name() {
    assert_eq!(
        zone_from_timezone_file("Asia/Shanghai\n").as_deref(),
        Some("Asia/Shanghai")
    );
    assert_eq!(zone_from_timezone_file("\n"), None);
}

#[test]
fn sysconfig_clock_yields_zone_name() {
    assert_eq!(
        zone_from_sysconfig_clock("UTC=false\nZONE=\"Asia/Shanghai\"\n").as_deref(),
        Some("Asia/Shanghai")
    );
    assert_eq!(
        zone_from_sysconfig_clock("ZONE=America/New_York").as_deref(),
        Some("America/New_York")
    );
    assert_eq!(zone_from_sysconfig_clock("UTC=true\n"), None);
}

#[test]
fn missing_timezone_sources_fall_back_to_utc_with_warning() {
    let conn = conn_with_files(&["/var/log/syslog"]);
    let mut reader = bytes_reader("");
    let (context, warnings) = resolve_linux_log_time(&conn, &AtomicBool::new(false), &mut reader);
    assert_eq!(context.tz_label(), "utc");
    assert!(warnings
        .iter()
        .any(|warning| warning == UTC_FALLBACK_WARNING));
    // UTC fallback is the identity conversion.
    let local = naive(2024, 1, 15, 10, 30, 0);
    assert_eq!(
        context.clock().local_to_utc(local).expect("utc timestamp"),
        DateTime::from_timestamp(1_705_314_600, 0).expect("valid epoch")
    );
}

#[test]
fn resolves_zone_from_localtime_symlink_text() {
    let conn = conn_with_files(&["/etc/localtime"]);
    let mut reader = bytes_reader("../usr/share/zoneinfo/Asia/Shanghai\n");
    let (context, warnings) = resolve_linux_log_time(&conn, &AtomicBool::new(false), &mut reader);
    assert_eq!(context.tz_label(), "Asia/Shanghai");
    assert!(!warnings
        .iter()
        .any(|warning| warning == UTC_FALLBACK_WARNING));
    // +08:00: naive 10:30 local lands at 02:30 UTC.
    let local = naive(2024, 1, 15, 10, 30, 0);
    assert_eq!(
        context
            .clock()
            .local_to_utc(local)
            .expect("timestamp")
            .to_rfc3339(),
        "2024-01-15T02:30:00+00:00"
    );
}

#[test]
fn resolves_zone_from_etc_timezone_when_localtime_absent() {
    let conn = conn_with_files(&["/etc/timezone"]);
    let mut reader = bytes_reader("Asia/Shanghai\n");
    let (context, _) = resolve_linux_log_time(&conn, &AtomicBool::new(false), &mut reader);
    assert_eq!(context.tz_label(), "Asia/Shanghai");
}

#[test]
fn resolves_zone_from_sysconfig_clock_when_others_absent() {
    let conn = conn_with_files(&["/etc/sysconfig/clock"]);
    let mut reader = bytes_reader("ZONE=\"Asia/Shanghai\"\n");
    let (context, _) = resolve_linux_log_time(&conn, &AtomicBool::new(false), &mut reader);
    assert_eq!(context.tz_label(), "Asia/Shanghai");
}

#[test]
fn unknown_zone_falls_through_to_next_source() {
    let conn = conn_with_files(&["/etc/localtime", "/etc/timezone"]);
    let mut reader = |candidate: &EvidenceCandidate, _| -> Result<CandidateSource, String> {
        let text = if candidate.path.ends_with("/etc/localtime") {
            "../usr/share/zoneinfo/Not/AZone"
        } else {
            "Asia/Shanghai\n"
        };
        Ok(CandidateSource::Bytes(text.as_bytes().to_vec()))
    };
    let (context, warnings) = resolve_linux_log_time(&conn, &AtomicBool::new(false), &mut reader);
    assert_eq!(context.tz_label(), "Asia/Shanghai");
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("unknown timezone 'Not/AZone'")));
}

#[test]
fn dst_zone_applies_historical_offsets() {
    let context = LinuxLogTimeContext::for_zone("America/New_York".parse().expect("valid zone"));
    assert_eq!(context.tz_label(), "America/New_York");
    // January: EST (UTC-5); July: EDT (UTC-4).
    let winter = context
        .clock()
        .local_to_utc(naive(2024, 1, 15, 12, 0, 0))
        .expect("winter timestamp");
    assert_eq!(winter.to_rfc3339(), "2024-01-15T17:00:00+00:00");
    let summer = context
        .clock()
        .local_to_utc(naive(2024, 7, 15, 12, 0, 0))
        .expect("summer timestamp");
    assert_eq!(summer.to_rfc3339(), "2024-07-15T16:00:00+00:00");
    // Autumn fold resolves to the earlier occurrence (EDT, UTC-4).
    let fold = context
        .clock()
        .local_to_utc(naive(2024, 11, 3, 1, 30, 0))
        .expect("fold timestamp");
    assert_eq!(fold.to_rfc3339(), "2024-11-03T05:30:00+00:00");
}
