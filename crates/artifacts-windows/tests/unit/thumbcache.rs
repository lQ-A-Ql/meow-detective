use super::*;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};

#[test]
fn thumbcache_supports_path() {
    let extractor = ThumbcacheExtractor;
    assert!(extractor
        .supports_path("C:/Users/test/AppData/Local/Microsoft/Windows/Explorer/thumbcache_32.db"));
    assert!(extractor.supports_path(
        "C:\\Users\\test\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_256.db"
    ));
    assert!(!extractor.supports_path("C:/Windows/System32/config/SYSTEM"));
    assert!(!extractor.supports_path("C:/test/file.txt"));
}

#[test]
fn thumbcache_invalid_data_no_panic() {
    let extractor = ThumbcacheExtractor;
    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("test".to_string()),
        file_path: "thumbcache_32.db".into(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 100])),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert_eq!(report.artifacts_found, 0);
    assert!(!report.errors.is_empty());
}

#[test]
fn thumbcache_valid_header() {
    let extractor = ThumbcacheExtractor;
    let mut data = vec![0u8; 1024];
    data[0..4].copy_from_slice(b"CMMM");
    data[4..8].copy_from_slice(&24u32.to_le_bytes());
    data[8..12].copy_from_slice(&1u32.to_le_bytes());
    data[12..16].copy_from_slice(&1u32.to_le_bytes());

    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("test".to_string()),
        file_path: "thumbcache_32.db".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert_eq!(report.artifacts_found, 1);
    assert_eq!(sink.artifacts[0].family, "Thumbcache");
}

#[test]
fn thumbcache_enumerates_entries() {
    let extractor = ThumbcacheExtractor;
    let entry_offset = 24u32;
    let mut data = vec![0u8; 100];
    data[0..4].copy_from_slice(b"CMMM");
    data[4..8].copy_from_slice(&24u32.to_le_bytes());
    data[8..12].copy_from_slice(&2u32.to_le_bytes());
    data[12..16].copy_from_slice(&1u32.to_le_bytes());
    data[16..20].copy_from_slice(&entry_offset.to_le_bytes());

    let off = entry_offset as usize;
    data[off..off + 4].copy_from_slice(&40u32.to_le_bytes());
    data[off + 4..off + 8].copy_from_slice(&0xCAFEBABEu32.to_le_bytes());
    data[off + 8..off + 12].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
    data[off + 12..off + 16].copy_from_slice(&24u32.to_le_bytes());

    let off2 = off + 40;
    data[off2..off2 + 4].copy_from_slice(&36u32.to_le_bytes());
    data[off2 + 4..off2 + 8].copy_from_slice(&0x9ABCDEF0u32.to_le_bytes());
    data[off2 + 8..off2 + 12].copy_from_slice(&0x12345678u32.to_le_bytes());
    data[off2 + 12..off2 + 16].copy_from_slice(&20u32.to_le_bytes());

    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("tc-entries".to_string()),
        file_path: "thumbcache_32.db".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert_eq!(report.artifacts_found, 1);
    assert!(report.errors.is_empty());

    let attrs = &sink.artifacts[0].attrs;
    assert_eq!(attrs.get("entry_count").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(
        attrs
            .get("total_thumbnail_data_size")
            .and_then(|v| v.as_u64()),
        Some(44)
    );

    let entries = attrs.get("entries").and_then(|v| v.as_array()).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["hash"], "CAFEBABEDEADBEEF");
    assert_eq!(entries[0]["data_size"], 24);
    assert_eq!(entries[1]["hash"], "9ABCDEF012345678");
    assert_eq!(entries[1]["data_size"], 20);
}
