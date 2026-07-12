use super::*;
use artifacts_core::VecSink;

#[test]
fn sru_supports_path() {
    let extractor = SruExtractor;
    assert!(extractor.supports_path("C:/Windows/System32/sru/SRUDB.DAT"));
    assert!(extractor.supports_path("C:\\Windows\\System32\\sru\\SRUDB.DAT"));
    assert!(!extractor.supports_path("C:/Windows/System32/config/SYSTEM"));
}

#[test]
fn sru_invalid_data_no_panic() {
    let extractor = SruExtractor;
    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("test".to_string()),
        file_path: "SRUDB.DAT".into(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 100])),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert_eq!(report.artifacts_found, 0);
    assert_eq!(report.errors, vec!["Not a recognized SRU ESE database"]);
}

#[test]
fn sru_sqlite_header_is_rejected() {
    let extractor = SruExtractor;
    let mut data = vec![0u8; 1024];
    data[0..16].copy_from_slice(b"SQLite format 3\0");

    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("test".to_string()),
        file_path: "SRUDB.DAT".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert_eq!(report.artifacts_found, 0);
    assert_eq!(
        report.errors,
        vec!["SRUDB.DAT uses ESE/Jet Blue format, not SQLite"]
    );
    assert!(sink.artifacts.is_empty());
}

#[test]
fn sru_ese_header_creates_file_level_artifact() {
    let extractor = SruExtractor;
    let mut data = vec![0u8; 1024];
    data[4..8].copy_from_slice(&0x89AB_CDEFu32.to_le_bytes());

    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("test".to_string()),
        file_path: "SRUDB.DAT".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert_eq!(report.artifacts_found, 1);
}

#[test]
fn sru_ese_header_extracts_page_size_and_state() {
    let extractor = SruExtractor;
    let mut data = vec![0u8; 8192];
    data[4..8].copy_from_slice(&0x89AB_CDEFu32.to_le_bytes());
    data[8..12].copy_from_slice(&0x0620u32.to_le_bytes());
    data[0x40..0x44].copy_from_slice(&4096u32.to_le_bytes());
    data[0x28..0x2C].copy_from_slice(&3u32.to_le_bytes());

    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("test".to_string()),
        file_path: "SRUDB.DAT".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert_eq!(report.artifacts_found, 1);
    let attrs = &sink.artifacts[0].attrs;
    assert_eq!(attrs.get("page_size").and_then(|v| v.as_u64()), Some(4096));
    assert_eq!(
        attrs.get("estimated_pages").and_then(|v| v.as_u64()),
        Some(2)
    );
    assert_eq!(
        attrs.get("database_state").and_then(|v| v.as_u64()),
        Some(3)
    );
    assert_eq!(
        attrs.get("database_state_desc").and_then(|v| v.as_str()),
        Some("CleanShutdown")
    );
}
