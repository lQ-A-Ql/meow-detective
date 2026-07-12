use super::*;
use artifacts_core::VecSink;

#[test]
fn jump_list_supports_path() {
    let extractor = JumpListExtractor;
    assert!(extractor.supports_path(
        "C:/Users/test/AppData/Roaming/Microsoft/Windows/Recent/5f7b5f7e3243a7b8.ms-abc"
    ));
    assert!(extractor.supports_path("C:/Users/test/AppData/Roaming/Microsoft/Windows/Recent/Custom/custom.customDestinations-ms"));
    assert!(!extractor.supports_path("C:/Users/test/file.txt"));
    assert!(!extractor.supports_path("C:/Users/test/file.lnk"));
}

#[test]
fn jump_list_truncated_no_panic() {
    let extractor = JumpListExtractor;
    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("test".to_string()),
        file_path: "test.ms-abc".into(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 10])),
    };
    let mut sink = VecSink::new();
    let _ = extractor.run(ctx, &mut sink);
}
