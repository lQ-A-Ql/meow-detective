use artifacts_core::{ArtifactContext, ArtifactExtractor, ExtractorRegistry, VecSink};
use artifacts_windows::{LnkExtractor, PrefetchExtractor, RecycleBinExtractor};
use domain::FileEntryId;

fn mini_prefetch() -> Vec<u8> {
    let mut data = Vec::new();
    // SCCA magic
    data.extend_from_slice(b"SCCA");
    // version
    data.extend_from_slice(&0x1Eu32.to_le_bytes());
    // executable name "cmd.exe" in UTF-16LE
    for c in "cmd.exe".encode_utf16() {
        data.extend_from_slice(&c.to_le_bytes());
    }
    data.extend_from_slice(&[0, 0]);
    // run count
    data.extend_from_slice(&5u32.to_le_bytes());
    // skip 4 bytes (reserved)
    data.extend_from_slice(&[0u8; 4]);
    // 8 FILETIME slots (first one = 2024-01-15T12:00:00Z)
    let ft = (133700000000000000u64).to_le_bytes();
    data.extend_from_slice(&ft);
    for _ in 1..8 {
        data.extend_from_slice(&[0u8; 8]);
    }
    data
}

fn mini_lnk() -> Vec<u8> {
    let mut data = Vec::new();
    // Header size (0x4C) + LNK magic
    data.extend_from_slice(&0x4Cu32.to_le_bytes());
    data.extend_from_slice(b"L\x00\x00\x00");
    // CLSID (16 zero bytes)
    data.extend_from_slice(&[0u8; 16]);
    // flags
    data.extend_from_slice(&0u32.to_le_bytes());
    // file attributes
    data.extend_from_slice(&0x20u32.to_le_bytes());
    // creation time
    let ft = (133700000000000000u64).to_le_bytes();
    data.extend_from_slice(&ft);
    // access time
    data.extend_from_slice(&ft);
    // write time
    data.extend_from_slice(&ft);
    // file size
    data.extend_from_slice(&1024u32.to_le_bytes());
    // icon index
    data.extend_from_slice(&0i32.to_le_bytes());
    // show command
    data.extend_from_slice(&1u32.to_le_bytes());
    // hotkey
    data.extend_from_slice(&[0u8; 2]);
    // reserved
    data.extend_from_slice(&[0u8; 10]);
    data
}

#[test]
fn prefetch_parser_produces_artifact() {
    let extractor = PrefetchExtractor;
    let ctx = ArtifactContext {
        file_id: FileEntryId("pf-001".into()),
        file_path: "cmd.exe-A1B2C3D4.pf".into(),
        reader: Box::new(std::io::Cursor::new(mini_prefetch())),
    };
    assert!(extractor.supports(&ctx));

    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert!(report.artifacts_found > 0);
    assert!(!sink.artifacts.is_empty());
    assert_eq!(sink.artifacts[0].family, "Prefetch");
}

#[test]
fn lnk_parser_produces_artifact() {
    let extractor = LnkExtractor;
    let ctx = ArtifactContext {
        file_id: FileEntryId("lnk-001".into()),
        file_path: "shortcut.lnk".into(),
        reader: Box::new(std::io::Cursor::new(mini_lnk())),
    };
    assert!(extractor.supports(&ctx));

    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert!(report.artifacts_found > 0);
    assert!(!sink.artifacts.is_empty());
}

#[test]
fn extractor_registry_works() {
    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(PrefetchExtractor));
    registry.register(Box::new(LnkExtractor));
    registry.register(Box::new(RecycleBinExtractor));

    let ctx = ArtifactContext {
        file_id: FileEntryId("pf-001".into()),
        file_path: "cmd.exe-A1B2C3D4.pf".into(),
        reader: Box::new(std::io::Cursor::new(mini_prefetch())),
    };

    let matches = registry.find_for(&ctx);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id(), "prefetch");
}

#[test]
fn unsupported_file_returns_empty() {
    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(PrefetchExtractor));
    registry.register(Box::new(LnkExtractor));

    let ctx = ArtifactContext {
        file_id: FileEntryId("txt-001".into()),
        file_path: "notes.txt".into(),
        reader: Box::new(std::io::Cursor::new(b"hello world".to_vec())),
    };

    let matches = registry.find_for(&ctx);
    assert!(matches.is_empty());
}
