use artifacts_core::{ArtifactContext, ArtifactExtractor, ExtractorRegistry, VecSink};
use artifacts_windows::{LnkExtractor, PrefetchExtractor, RecycleBinExtractor};
use domain::FileEntryId;

fn mini_prefetch() -> Vec<u8> {
    let mut data = Vec::new();
    // SCCA magic
    data.extend_from_slice(b"SCCA");
    // format version (v30 = 0x1E)
    data.extend_from_slice(&0x1Eu32.to_le_bytes());
    // signature
    data.extend_from_slice(&0u32.to_le_bytes());
    // unused
    data.extend_from_slice(&0u32.to_le_bytes());
    // file_size
    data.extend_from_slice(&1024u32.to_le_bytes());
    // executable name "cmd.exe" in UTF-16LE (60 bytes / 30 chars)
    let mut name_buf = vec![0u8; 60];
    for (i, c) in "cmd.exe".encode_utf16().enumerate() {
        let bytes = c.to_le_bytes();
        name_buf[i * 2] = bytes[0];
        name_buf[i * 2 + 1] = bytes[1];
    }
    data.extend_from_slice(&name_buf);
    // hash
    data.extend_from_slice(&0xA1B2C3D4u32.to_le_bytes());
    // flags
    data.extend_from_slice(&0u32.to_le_bytes());
    // skip 12 bytes before run count (v30)
    data.extend_from_slice(&[0u8; 12]);
    // run count
    data.extend_from_slice(&5u32.to_le_bytes());
    // 8 FILETIME slots (first one ≈ 2024-01-15)
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
    assert!(extractor.supports_path("cmd.exe-A1B2C3D4.pf"));

    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink).unwrap();
    assert!(report.artifacts_found > 0);
    assert!(!sink.artifacts.is_empty());
}

#[test]
fn lnk_parser_produces_artifact() {
    let extractor = LnkExtractor;
    assert!(extractor.supports_path("shortcut.lnk"));

    let ctx = ArtifactContext {
        file_id: FileEntryId("lnk-001".into()),
        file_path: "shortcut.lnk".into(),
        reader: Box::new(std::io::Cursor::new(mini_lnk())),
    };
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

    let matches = registry.find_for_path("cmd.exe-A1B2C3D4.pf");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id(), "prefetch");
}

#[test]
fn unsupported_file_returns_empty() {
    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(PrefetchExtractor));
    registry.register(Box::new(LnkExtractor));

    let matches = registry.find_for_path("notes.txt");
    assert!(matches.is_empty());
}

#[test]
fn prefetch_truncated_no_panic() {
    let extractor = PrefetchExtractor;
    let ctx = ArtifactContext {
        file_id: FileEntryId("trunc".into()),
        file_path: "bad.pf".into(),
        reader: Box::new(std::io::Cursor::new(vec![0xAB, 0xCD])),
    };
    let mut sink = VecSink::new();
    let result = extractor.run(ctx, &mut sink);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn lnk_truncated_no_panic() {
    let extractor = LnkExtractor;
    let ctx = ArtifactContext {
        file_id: FileEntryId("trunc".into()),
        file_path: "bad.lnk".into(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 10])),
    };
    let mut sink = VecSink::new();
    let result = extractor.run(ctx, &mut sink);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn registry_random_bytes_no_panic() {
    let extractor = artifacts_windows::RegistryExtractor;
    let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    let ctx = ArtifactContext {
        file_id: FileEntryId("rand".into()),
        file_path: "C:/Windows/System32/config/SYSTEM.dat".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let result = extractor.run(ctx, &mut sink);
    assert!(result.is_ok() || result.is_err());
}
