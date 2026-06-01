mod fixture_builder;

use artifacts_core::{ArtifactContext, ArtifactExtractor, ExtractorRegistry, VecSink};
use artifacts_windows::{LnkExtractor, PrefetchExtractor, RecycleBinExtractor, RegistryExtractor};
use chrono::{TimeZone, Utc};
use domain::FileEntryId;
use fixture_builder::*;

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
fn registry_supports_standard_extensionless_hives_only_in_config_path() {
    let extractor = RegistryExtractor;

    assert!(extractor.supports_path("C:/Windows/System32/config/SYSTEM"));
    assert!(extractor.supports_path("Windows\\System32\\config\\SOFTWARE"));
    assert!(extractor.supports_path("C:/Users/alice/NTUSER.DAT"));
    assert!(extractor.supports_path("C:/Users/alice/AppData/Local/Microsoft/Windows/UsrClass.dat"));
    assert!(!extractor.supports_path("C:/Temp/SYSTEM"));
    assert!(!extractor.supports_path("C:/Temp/software"));
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
