use super::*;
use crate::analysis_service::extraction::reader::CandidateSource;
crate::native_source_test_support!();
use chrono::NaiveDate;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use std::sync::atomic::AtomicBool;
use test_support::NativeSourceFixture;

fn naive(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d)
        .and_then(|date| date.and_hms_opt(hh, mm, ss))
        .expect("valid naive datetime")
}

fn conn_with_files(paths: &[&str]) -> NativeSourceFixture {
    let fixture = NativeSourceFixture::new("ds-tz", "case-tz");
    let files = paths
        .iter()
        .enumerate()
        .map(|(index, path)| FileEntry {
            id: FileEntryId(format!("tz-file-{index}")),
            parent_id: None,
            data_source_id: DataSourceId("ds-tz".into()),
            path: (*path).into(),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            entry_type: EntryType::File,
            size: Some(128),
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        })
        .collect::<Vec<_>>();
    fixture.insert_files(&files);
    fixture.seal();
    fixture
}

fn bytes_reader(
    content: &'static str,
) -> impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String> {
    move |_, _| Ok(CandidateSource::Bytes(content.as_bytes().to_vec()))
}

fn preparation_catalog(conn: &rusqlite::Connection) -> PreparationCatalog {
    crate::analysis_service::candidates::evidence_candidates_for_categories_with_preparation(
        conn,
        &["LinuxArtifacts"],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap()
    .2
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
    let (context, warnings) = resolve_linux_log_time(
        &preparation_catalog(&conn),
        &AtomicBool::new(false),
        &mut reader,
    );
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
    let (context, warnings) = resolve_linux_log_time(
        &preparation_catalog(&conn),
        &AtomicBool::new(false),
        &mut reader,
    );
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
    let (context, _) = resolve_linux_log_time(
        &preparation_catalog(&conn),
        &AtomicBool::new(false),
        &mut reader,
    );
    assert_eq!(context.tz_label(), "Asia/Shanghai");
}

#[test]
fn resolves_zone_from_sysconfig_clock_when_others_absent() {
    let conn = conn_with_files(&["/etc/sysconfig/clock"]);
    let mut reader = bytes_reader("ZONE=\"Asia/Shanghai\"\n");
    let (context, _) = resolve_linux_log_time(
        &preparation_catalog(&conn),
        &AtomicBool::new(false),
        &mut reader,
    );
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
    let (context, warnings) = resolve_linux_log_time(
        &preparation_catalog(&conn),
        &AtomicBool::new(false),
        &mut reader,
    );
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

#[test]
fn timezone_files_are_resolved_for_each_partition() {
    use persistence_source::fjall::{CatalogEntryType, FileEntryRecord};
    let workspace =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codex-tmp/timezone-tests");
    std::fs::create_dir_all(&workspace).unwrap();
    let root = tempfile::tempdir_in(workspace).unwrap();
    std::fs::write(root.path().join("zone-one"), b"UTC\n").unwrap();
    std::fs::write(root.path().join("zone-two"), b"Asia/Shanghai\n").unwrap();
    let mut catalog = PreparationCatalog::default();
    for (id, partition, size) in [("zone-one", 1, 4), ("zone-two", 2, 14)] {
        catalog
            .observe(&FileEntryRecord {
                id: id.into(),
                parent_id: None,
                data_source_id: "source".into(),
                path: format!("[P{partition}]/etc/timezone"),
                name: "timezone".into(),
                entry_type: CatalogEntryType::File,
                size: Some(size),
                ext: None,
                deleted: false,
                hidden: false,
                system: false,
                read_only: false,
                archive: false,
                encrypted: Some(false),
                created: None,
                modified: None,
                accessed: None,
                changed: None,
                hash_sha256: None,
                partition_index: Some(partition),
                unix_mode: None,
            })
            .unwrap();
    }
    let (context, _) =
        resolve_linux_log_time(&catalog, &AtomicBool::new(false), &mut |candidate, _| {
            std::fs::File::open(root.path().join(&candidate.file_id.0))
                .map(|reader| CandidateSource::Reader(Box::new(reader)))
                .map_err(|error| error.to_string())
        });
    for (partition, expected) in [
        (1, "2024-01-15T10:30:00+00:00"),
        (2, "2024-01-15T02:30:00+00:00"),
    ] {
        let candidate = EvidenceCandidate {
            file_id: FileEntryId("log".into()),
            data_source_id: "source".into(),
            partition_index: Some(partition),
            path: "/var/log/dpkg.log".into(),
            size: 1,
            encrypted: false,
            content_identity: "log".into(),
            companions: Vec::new(),
            evidence_kind: "log".into(),
            parser: "linux.apt".into(),
            category: "LinuxArtifacts".into(),
            modified_at: None,
        };
        assert_eq!(
            context
                .for_candidate(&candidate)
                .clock()
                .local_to_utc(naive(2024, 1, 15, 10, 30, 0))
                .unwrap()
                .to_rfc3339(),
            expected
        );
    }
}
