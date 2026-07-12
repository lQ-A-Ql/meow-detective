use super::*;
use artifacts_core::VecSink;
use domain::FileEntryId;

#[test]
fn mam_prefetch_without_payload_fails_closed() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MAM\x04");
    bytes.extend_from_slice(&4096u32.to_le_bytes());
    bytes.resize(128, 0);

    let ctx = ArtifactContext {
        file_id: FileEntryId("pf-1".to_string()),
        file_path: "C:/Windows/Prefetch/CMD.EXE-1234.pf".to_string(),
        reader: Box::new(std::io::Cursor::new(bytes)),
    };
    let mut sink = VecSink::new();

    let report = PrefetchExtractor.run(ctx, &mut sink).unwrap();

    assert_eq!(report.artifacts_found, 0);
    assert_eq!(report.timeline_events, 0);
    assert_eq!(report.errors.len(), 1);
    assert!(sink.artifacts.is_empty());
}
