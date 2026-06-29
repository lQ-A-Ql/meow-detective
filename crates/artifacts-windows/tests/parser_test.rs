mod fixture_builder;

use artifacts_core::{ArtifactContext, ArtifactExtractor, ExtractorRegistry, VecSink};
use artifacts_windows::{
    extract_boot_shutdown_events, extract_boot_shutdown_events_from_json_records,
    JumpListExtractor, LnkExtractor, PrefetchExtractor, RecycleBinExtractor, RegistryExtractor,
    SruExtractor, ThumbcacheExtractor,
};
use chrono::{TimeZone, Utc};
use domain::FileEntryId;
use fixture_builder::*;
use serde_json::json;

fn t(s: &str) -> FileEntryId {
    FileEntryId(s.to_string())
}

// --- Robustness tests ---

#[test]
fn prefetch_truncated_no_panic() {
    let extractor = PrefetchExtractor;
    let ctx = ArtifactContext {
        file_id: t("trunc"),
        file_path: "bad.pf".into(),
        reader: Box::new(std::io::Cursor::new(vec![0xAB, 0xCD])),
    };
    let mut sink = VecSink::new();
    let _ = extractor.run(ctx, &mut sink);
}

#[test]
fn lnk_truncated_no_panic() {
    let extractor = LnkExtractor;
    let ctx = ArtifactContext {
        file_id: t("trunc"),
        file_path: "bad.lnk".into(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 10])),
    };
    let mut sink = VecSink::new();
    let _ = extractor.run(ctx, &mut sink);
}

#[test]
fn registry_random_bytes_no_panic() {
    let extractor = RegistryExtractor;
    let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    let ctx = ArtifactContext {
        file_id: t("rand"),
        file_path: "C:/Windows/System32/config/SYSTEM.dat".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let _ = extractor.run(ctx, &mut sink);
}

// --- Format-correct fixture tests ---

#[test]
fn prefetch_fixture_extracts_exe_and_runs() {
    let t1 = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2024, 1, 16, 8, 30, 0).unwrap();
    let data = build_prefetch_v30("CMD.EXE", 5, &[t1, t2]);

    let extractor = PrefetchExtractor;
    assert!(extractor.supports_path("CMD.EXE-DEADBEEF.pf"));
    let ctx = ArtifactContext {
        file_id: t("pf-001"),
        file_path: "CMD.EXE-DEADBEEF.pf".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert!(report.artifacts_found > 0);
    assert!(!sink.timeline_events.is_empty(), "Expected timeline events");
    assert_eq!(sink.artifacts[0].family, "Prefetch");
    assert!(sink.artifacts[0].title.contains("CMD.EXE"));
}

#[test]
fn mam_prefetch_fails_closed_without_decompression() {
    let mut data = build_prefetch_v30("CMD.EXE", 5, &[]);
    data[0..4].copy_from_slice(b"MAM\x04");
    data[4..8].copy_from_slice(&(4096u32).to_le_bytes());

    let extractor = PrefetchExtractor;
    let ctx = ArtifactContext {
        file_id: t("pf-mam"),
        file_path: "CMD.EXE-DEADBEEF.pf".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 0);
    assert_eq!(report.timeline_events, 0);
    assert_eq!(report.errors.len(), 1);
    assert!(sink.artifacts.is_empty());
}

#[cfg(windows)]
#[test]
fn mam_prefetch_fixture_extracts_exe_and_runs() {
    let t1 = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2024, 1, 16, 8, 30, 0).unwrap();
    let data = build_prefetch_mam_v30("CMD.EXE", 5, &[t1, t2]);

    let extractor = PrefetchExtractor;
    let ctx = ArtifactContext {
        file_id: t("pf-mam-real"),
        file_path: "CMD.EXE-DEADBEEF.pf".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 1);
    assert_eq!(sink.artifacts[0].family, "Prefetch");
    assert!(sink.artifacts[0].title.contains("CMD.EXE"));
    assert_eq!(
        sink.artifacts[0]
            .attrs
            .get("run_count")
            .and_then(|value| value.as_u64()),
        Some(5)
    );
    assert_eq!(report.timeline_events, 2);
}

#[test]
fn prefetch_utf16_name_uses_aligned_nul_terminator() {
    let data = build_prefetch_v30("CMD.EXE", 5, &[]);

    let extractor = PrefetchExtractor;
    let ctx = ArtifactContext {
        file_id: t("pf-utf16"),
        file_path: "CMD.EXE-DEADBEEF.pf".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 1);
    assert!(sink.artifacts[0].title.contains("CMD.EXE"));
}

#[test]
fn lnk_fixture_extracts_target_path() {
    let ct = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
    let wt = Utc.with_ymd_and_hms(2024, 1, 16, 8, 0, 0).unwrap();
    let data = build_lnk(
        Some("C:\\Windows\\System32\\cmd.exe"),
        Some(ct),
        Some(wt),
        1024,
    );

    let extractor = LnkExtractor;
    assert!(extractor.supports_path("shortcut.lnk"));
    let ctx = ArtifactContext {
        file_id: t("lnk-001"),
        file_path: "shortcut.lnk".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert!(report.artifacts_found > 0);
    let attrs = &sink.artifacts[0].attrs;
    let tp = attrs
        .get("target_path")
        .map(|v| v.as_str().unwrap_or(""))
        .unwrap_or("");
    assert!(
        tp.contains("cmd.exe"),
        "Expected cmd.exe in target_path, got: '{}', attrs: {:?}",
        tp,
        attrs
    );
}

#[test]
fn recycle_bin_fixture_extracts_path_and_time() {
    let dt = Utc.with_ymd_and_hms(2024, 6, 15, 10, 30, 0).unwrap();
    let data = build_recycle_bin_i("C:\\Users\\alice\\Documents\\secret.docx", 65536, dt);

    let extractor = RecycleBinExtractor;
    assert!(extractor.supports_path("$Recycle.Bin/$IA1B2C3D4E5.exe"));
    let ctx = ArtifactContext {
        file_id: t("rb-001"),
        file_path: "$Recycle.Bin/$IA1B2C3D4E5.exe".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert!(report.artifacts_found > 0);
    let attrs = &sink.artifacts[0].attrs;
    assert!(
        attrs.contains_key("original_path"),
        "Expected original_path in attrs"
    );
    assert!(
        attrs
            .get("original_path")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("secret"),
        "Expected path to contain 'secret'"
    );
    assert!(
        !sink.timeline_events.is_empty(),
        "Expected FILE_DELETED event"
    );
}

#[test]
fn registry_hive_fixture_parses_name() {
    let lw = Utc.with_ymd_and_hms(2025, 3, 1, 18, 0, 0).unwrap();
    let data = build_registry_hive("SYSTEM", lw);

    let extractor = RegistryExtractor;
    let ctx = ArtifactContext {
        file_id: t("reg-001"),
        file_path: "C:/Windows/System32/config/SYSTEM.dat".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert!(report.artifacts_found > 0);
    let attrs = &sink.artifacts[0].attrs;
    assert!(
        attrs.contains_key("hive_name"),
        "Expected hive_name in attrs: {:?}",
        attrs
    );
    assert!(
        !sink.timeline_events.is_empty(),
        "Expected REGISTRY_MODIFIED event"
    );
}

#[test]
fn evtx_json_records_extract_boot_shutdown_candidates() {
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[
            json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "EventLog" },
                        "EventID": 6005,
                        "EventRecordID": 10,
                        "TimeCreated": { "@SystemTime": "2026-02-01T00:00:00Z" }
                    }
                }
            }),
            json!({
                "Event": {
                    "System": {
                        "Provider": "EventLog",
                        "EventID": { "#text": "6008" },
                        "EventRecordID": "11",
                        "TimeCreated": "2026-02-01 01:30:00 +0000"
                    }
                }
            }),
            json!({
                "Event": {
                    "System": {
                        "Provider": { "Name": "User32" },
                        "EventID": "1074",
                        "EventRecordID": 12,
                        "TimeCreated": { "SystemTime": "2026-02-01T02:00:00Z" }
                    }
                }
            }),
            json!({"Event":{"System":{"EventID":7045}}}),
        ],
        "Windows/System32/winevt/Logs/System.evtx",
    )
    .expect("json extraction should succeed");

    assert!(extraction.warnings.is_empty());
    assert_eq!(extraction.events.len(), 3);
    assert_eq!(extraction.events[0].event_id, 6005);
    assert_eq!(extraction.events[0].kind.as_str(), "eventLogStarted");
    assert_eq!(extraction.events[0].timestamp, "2026-02-01T00:00:00+00:00");
    assert_eq!(extraction.events[0].record_id, Some(10));
    assert_eq!(extraction.events[0].provider.as_deref(), Some("EventLog"));
    assert_eq!(extraction.events[1].event_id, 6008);
    assert_eq!(extraction.events[1].kind.as_str(), "unexpectedShutdown");
    assert_eq!(extraction.events[1].record_id, Some(11));
    assert!(extraction.events[1]
        .note
        .contains("unexpected prior shutdown"));
    assert_eq!(extraction.events[2].event_id, 1074);
    assert_eq!(extraction.events[2].kind.as_str(), "plannedShutdown");
    assert_eq!(extraction.events[2].provider.as_deref(), Some("User32"));
}

#[test]
fn evtx_json_records_fail_closed_for_unsupported_log_path() {
    let result = extract_boot_shutdown_events_from_json_records(
        &[json!({
            "Event": {
                "System": {
                    "Provider": { "@Name": "EventLog" },
                    "EventID": 6005,
                    "TimeCreated": { "@SystemTime": "2026-02-01T00:00:00Z" }
                }
            }
        })],
        "Windows/Temp/UnknownChannel.evtx",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("outside bounded EVTX parser scope"));
}

#[test]
fn evtx_binary_parser_fail_closed_for_unsupported_log_path() {
    let result = extract_boot_shutdown_events(b"ElfFile\0", "Windows/Temp/UnknownChannel.evtx");

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("outside bounded EVTX parser scope"));
}

#[test]
fn jumplist_no_embedded_lnk_reports_generic_artifact() {
    let extractor = JumpListExtractor;
    assert!(extractor.supports_path(
        "C:/Users/alice/AppData/Roaming/Microsoft/Windows/Recent/AutomaticDestinations/5f7b5f7e3243a7b8.ms-abc"
    ));
    assert!(extractor.supports_path(
        "C:/Users/alice/AppData/Roaming/Microsoft/Windows/Recent/CustomDestinations/example.customDestinations-ms"
    ));
    assert!(!extractor.supports_path("C:/Users/alice/Documents/message.msg"));
    assert!(!extractor.supports_path("C:/Users/alice/Documents/shortcut.lnk"));

    let data = b"valid-looking AutomaticDestinations data without embedded shell links".to_vec();
    let expected_size = data.len() as u64;
    let ctx = ArtifactContext {
        file_id: t("jl-001"),
        file_path: "5f7b5f7e3243a7b8.ms-abc".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 1);
    assert!(report.errors.is_empty());
    assert!(sink.timeline_events.is_empty());
    assert_eq!(sink.artifacts.len(), 1);
    assert_eq!(sink.artifacts[0].family, "JumpList");
    assert!(sink.artifacts[0].title.contains("5f7b5f7e3243a7b8.ms-abc"));
    assert_eq!(
        sink.artifacts[0]
            .attrs
            .get("format")
            .and_then(|v| v.as_str()),
        Some("AutomaticDestinations")
    );
    assert_eq!(
        sink.artifacts[0]
            .attrs
            .get("file_size")
            .and_then(|v| v.as_u64()),
        Some(expected_size)
    );
}

#[test]
fn thumbcache_minimal_header_extracts_metadata_artifact() {
    let extractor = ThumbcacheExtractor;
    assert!(extractor
        .supports_path("C:/Users/alice/AppData/Local/Microsoft/Windows/Explorer/thumbcache_32.db"));
    assert!(extractor.supports_path(
        "C:/Users/alice/AppData/Local/Microsoft/Windows/Explorer/thumbcache_256.db"
    ));
    assert!(!extractor
        .supports_path("C:/Users/alice/AppData/Local/Microsoft/Windows/Explorer/iconcache_32.db"));

    let mut data = vec![0u8; 24];
    data[0..4].copy_from_slice(b"CMMM");
    data[4..8].copy_from_slice(&24u32.to_le_bytes());
    data[8..12].copy_from_slice(&1u32.to_le_bytes());
    data[12..16].copy_from_slice(&3u32.to_le_bytes());

    let ctx = ArtifactContext {
        file_id: t("tc-001"),
        file_path: "thumbcache_256.db".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 1);
    assert!(report.errors.is_empty());
    assert_eq!(sink.artifacts.len(), 1);
    assert_eq!(sink.artifacts[0].family, "Thumbcache");
    let attrs = &sink.artifacts[0].attrs;
    assert_eq!(attrs.get("file_size").and_then(|v| v.as_u64()), Some(24));
    assert_eq!(attrs.get("header_size").and_then(|v| v.as_u64()), Some(24));
    assert_eq!(attrs.get("version").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(attrs.get("cache_type").and_then(|v| v.as_u64()), Some(3));
    assert_eq!(
        attrs.get("cache_type_desc").and_then(|v| v.as_str()),
        Some("256x256")
    );
}

#[test]
fn sru_invalid_minimal_content_reports_sqlite_error_without_artifacts() {
    let extractor = SruExtractor;
    assert!(extractor.supports_path("C:/Windows/System32/sru/SRUDB.DAT"));
    assert!(extractor.supports_path("C:\\Windows\\System32\\sru\\SRUDB.DAT"));
    assert!(!extractor.supports_path("C:/Windows/System32/config/SYSTEM"));

    let ctx = ArtifactContext {
        file_id: t("sru-001"),
        file_path: "C:/Windows/System32/sru/SRUDB.DAT".into(),
        reader: Box::new(std::io::Cursor::new(b"not sqlite".to_vec())),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 0);
    assert_eq!(report.timeline_events, 0);
    assert!(sink.artifacts.is_empty());
    assert!(sink.timeline_events.is_empty());
    assert_eq!(report.errors, vec!["Not a recognized SRU ESE database"]);
}

#[test]
fn sru_sqlite_header_reports_format_mismatch_without_artifacts() {
    let extractor = SruExtractor;
    let mut data = vec![0u8; 1024];
    data[0..16].copy_from_slice(b"SQLite format 3\0");

    let ctx = ArtifactContext {
        file_id: t("sru-sqlite"),
        file_path: "C:/Windows/System32/sru/SRUDB.DAT".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 0);
    assert_eq!(report.timeline_events, 0);
    assert!(sink.artifacts.is_empty());
    assert_eq!(
        report.errors,
        vec!["SRUDB.DAT uses ESE/Jet Blue format, not SQLite"]
    );
}

#[test]
fn sru_ese_header_reports_file_level_artifact() {
    let extractor = SruExtractor;
    let mut data = vec![0u8; 1024];
    data[4..8].copy_from_slice(&0x89AB_CDEFu32.to_le_bytes());

    let ctx = ArtifactContext {
        file_id: t("sru-ese"),
        file_path: "C:/Windows/System32/sru/SRUDB.DAT".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 1);
    assert!(report.errors.is_empty());
    assert_eq!(sink.artifacts.len(), 1);
    assert_eq!(sink.artifacts[0].family, "SRU");
}

#[test]
fn registry_supports_standard_extensionless_hives_only_in_config_path() {
    let extractor = RegistryExtractor;

    assert!(extractor.supports_path("C:/Windows/System32/config/SYSTEM"));
    assert!(extractor.supports_path("Windows\\System32\\config\\SOFTWARE"));
    assert!(extractor.supports_path("C:/Users/alice/NTUSER.DAT"));
    assert!(extractor.supports_path("C:/Users/alice/AppData/Local/Microsoft/Windows/UsrClass.dat"));
    assert!(!extractor.supports_path("C:/Temp/SYSTEM"));
    assert!(!extractor.supports_path("C:/Temp/software"));
    assert!(!extractor.supports_path("C:/Temp/system-backup.dat"));
    assert!(!extractor.supports_path("C:/Temp/awesome.dat"));
    assert!(!extractor.supports_path("notes.txt"));
}

#[test]
fn extractor_registry_matches_by_path() {
    let mut reg = ExtractorRegistry::new();
    reg.register(Box::new(PrefetchExtractor));
    reg.register(Box::new(LnkExtractor));
    reg.register(Box::new(RecycleBinExtractor));

    assert_eq!(reg.find_for_path("CMD.EXE-1234.pf").len(), 1);
    assert_eq!(reg.find_for_path("shortcut.lnk").len(), 1);
    assert_eq!(reg.find_for_path("$Recycle.Bin/$Iabc.exe").len(), 1);
    assert_eq!(reg.find_for_path("notes.txt").len(), 0);
}
