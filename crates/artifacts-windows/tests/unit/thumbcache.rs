use super::*;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};

use crate::tests::parser_fixtures as fixtures;

#[test]
fn thumbcache_supports_path() {
    let extractor = ThumbcacheExtractor;
    assert!(extractor
        .supports_path("C:/Users/test/AppData/Local/Microsoft/Windows/Explorer/thumbcache_32.db"));
    assert!(extractor.supports_path(
        "C:\\Users\\test\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_256.db"
    ));
    assert!(!extractor.supports_path("C:/Windows/System32/config/SYSTEM"));
    assert!(!extractor.supports_path("C:/test/thumbcache_idx.db"));
}

#[test]
fn thumbcache_invalid_data_is_rejected() {
    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("invalid".into()),
        file_path: "thumbcache_32.db".into(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 100])),
    };
    let mut sink = VecSink::new();
    assert!(ThumbcacheExtractor.run(ctx, &mut sink).is_err());
    assert!(sink.artifacts.is_empty());
}

#[test]
fn thumbcache_versions_and_entries_follow_independent_oracle() {
    for (name, version, cache_type) in [("thumbcache_v21.db", 21, 2), ("thumbcache_v32.db", 32, 4)]
    {
        let ctx = ArtifactContext {
            file_id: domain::FileEntryId(name.into()),
            file_path: "thumbcache_256.db".into(),
            reader: Box::new(std::io::Cursor::new(fixtures::bytes(name))),
        };
        let mut sink = VecSink::new();
        let report = ThumbcacheExtractor
            .run(ctx, &mut sink)
            .expect("decode public cache");
        assert_eq!(report.artifacts_found, 1);
        assert_eq!(
            report.errors.len(),
            2,
            "two intentionally damaged checksum entries"
        );
        let attrs = &sink.artifacts[0].attrs;
        assert_eq!(attrs["version"], version);
        assert_eq!(attrs["cache_type"], cache_type);
        assert_eq!(attrs["entry_count"], 8);
        assert_eq!(attrs["total_thumbnail_data_size"], 2597);
        assert_eq!(attrs["entries"][0]["hash"], "1111000000000001");
        assert_eq!(attrs["entries"][0]["data_size"], 102);
        assert_eq!(attrs["entries"][6]["data_checksum_verified"], false);
        assert_eq!(attrs["entries"][7]["header_checksum_verified"], false);
    }
}
