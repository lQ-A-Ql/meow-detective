use super::*;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};

use crate::tests::parser_fixtures as fixtures;

fn context(data: Vec<u8>) -> ArtifactContext {
    ArtifactContext {
        file_id: domain::FileEntryId("sru".into()),
        file_path: "Windows/System32/sru/SRUDB.dat".into(),
        reader: Box::new(std::io::Cursor::new(data)),
    }
}

#[test]
fn sru_supports_path() {
    assert!(SruExtractor.supports_path("C:/Windows/System32/sru/SRUDB.DAT"));
    assert!(SruExtractor.supports_path("C:\\Windows\\System32\\sru\\SRUDB.DAT"));
    assert!(!SruExtractor.supports_path("C:/Windows/System32/config/SYSTEM"));
}

#[test]
fn sru_invalid_data_is_rejected() {
    let mut sink = VecSink::new();
    assert!(SruExtractor
        .run(context(vec![0u8; 100]), &mut sink)
        .is_err());
    assert!(sink.artifacts.is_empty());
}

#[test]
fn sru_sqlite_header_is_rejected() {
    let mut data = vec![0u8; 1024];
    data[..16].copy_from_slice(b"SQLite format 3\0");
    let mut sink = VecSink::new();
    assert!(SruExtractor.run(context(data), &mut sink).is_err());
    assert!(sink.artifacts.is_empty());
}

#[test]
fn sru_public_database_exposes_typed_header_metadata() {
    let bytes = fixtures::bytes("SRUDB.dat.gz");
    let mut sink = VecSink::new();
    let report = SruExtractor
        .run(context(bytes.clone()), &mut sink)
        .expect("extract SRU metadata");
    assert_eq!(report.artifacts_found, 1);
    let attrs = &sink.artifacts[0].attrs;
    assert_eq!(attrs["metadata_only"], true);
    assert_eq!(attrs["ese_db_version"], 0x620);
    assert_eq!(attrs["page_size"], 4096);
    assert_eq!(attrs["estimated_pages"], bytes.len() as u64 / 4096);
    assert_eq!(attrs["database_state"], 3);
    assert_eq!(attrs["database_state_desc"], "CleanShutdown");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}
