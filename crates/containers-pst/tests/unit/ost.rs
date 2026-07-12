use super::*;

fn build_synthetic_ost() -> Vec<u8> {
    crate::tests::build_synthetic_unicode_pst()
}

#[test]
fn ost_opens_valid_ndb_file() {
    let data = build_synthetic_ost();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.ost");
    std::fs::write(&path, &data).unwrap();
    let reader = OstReader::open(&path).unwrap();
    assert!(reader.is_unicode());
    assert!(reader.file_size() > 0);
}

#[test]
fn ost_file_kind_detected_by_extension() {
    let data = build_synthetic_ost();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.ost");
    std::fs::write(&path, &data).unwrap();
    assert_eq!(
        OstReader::open(&path).unwrap().file_kind(),
        OutlookFileKind::Ost
    );
}

#[test]
fn ost_file_kind_defaults_to_pst_for_pst_extension() {
    let data = build_synthetic_ost();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pst");
    std::fs::write(&path, &data).unwrap();
    assert_eq!(
        OstReader::open(&path).unwrap().file_kind(),
        OutlookFileKind::Pst
    );
}

#[test]
fn ost_reader_delegates_to_pst() {
    let data = build_synthetic_ost();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.ost");
    std::fs::write(&path, &data).unwrap();
    let folders = OstReader::open(&path).unwrap().read_folders().unwrap();
    assert!(!folders.is_empty(), "Should have at least one folder");
}

#[test]
fn ost_ost_properties_accessible() {
    let data = build_synthetic_ost();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.ost");
    std::fs::write(&path, &data).unwrap();
    assert!(!OstReader::open(&path).unwrap().ost_properties().encrypted);
}

#[test]
fn ost_rejects_invalid_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.ost");
    std::fs::write(&path, b"not an ost file").unwrap();
    assert!(OstReader::open(&path).is_err());
}
