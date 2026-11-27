use super::*;
use std::fs::{File, FileTimes, OpenOptions};
use std::io::Write;

#[test]
fn verified_content_checks_partial_reads_final_chunk_seek_and_retains_verified_bytes() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("content.raw");
    let seed = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/public-small/mdraid/oracle.bin"),
    )
    .unwrap();
    let mut expected = seed.repeat(5);
    expected.truncate(CONTENT_CHUNK_BYTES + 257);
    File::create(&path).unwrap().write_all(&expected).unwrap();
    let original_time = path.metadata().unwrap().modified().unwrap();
    let content = hash_source(
        path.to_str().unwrap(),
        &DataSourceKind::Raw,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(content.chunk_sha256.len(), 2);
    assert_eq!(content.logical_size, expected.len() as u64);
    assert_eq!(
        content.logical_sha256,
        crate::hash_service::HashService::sha256_file(&path).unwrap()
    );
    let source = datasource_service::open_evidence_reader(&path, &DataSourceKind::Raw).unwrap();
    let mut verified =
        reader::ContentVerifiedReader::new(source, Arc::new(content.clone())).unwrap();
    verified
        .seek(SeekFrom::Start(CONTENT_CHUNK_BYTES as u64 - 7))
        .unwrap();
    let mut bytes = [0u8; 264];
    verified.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, expected[CONTENT_CHUNK_BYTES - 7..]);
    assert_eq!(verified.read(&mut bytes).unwrap(), 0);
    assert!(verified.seek(SeekFrom::Current(-i64::MAX)).is_err());
    let mut changed = OpenOptions::new().write(true).open(&path).unwrap();
    changed
        .seek(SeekFrom::Start(CONTENT_CHUNK_BYTES as u64 + 3))
        .unwrap();
    changed
        .write_all(&[expected[CONTENT_CHUNK_BYTES + 3] ^ 0x80])
        .unwrap();
    changed.sync_all().unwrap();
    changed
        .set_times(FileTimes::new().set_modified(original_time))
        .unwrap();
    drop(changed);
    verified.seek(SeekFrom::End(-257)).unwrap();
    let mut cached = [0; 257];
    verified.read_exact(&mut cached).unwrap();
    assert_eq!(cached, expected[CONTENT_CHUNK_BYTES..]);
    let source = datasource_service::open_evidence_reader(&path, &DataSourceKind::Raw).unwrap();
    let mut changed = reader::ContentVerifiedReader::new(source, Arc::new(content)).unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    let mut untouched = [0x5a];
    assert_eq!(
        changed.read(&mut untouched).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(untouched, [0x5a]);
}

#[test]
fn source_content_hashing_rejects_cancellation_and_accepts_empty_raw_images() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("empty.raw");
    File::create(&path).unwrap();
    assert!(matches!(
        hash_source(
            path.to_str().unwrap(),
            &DataSourceKind::Raw,
            &AtomicBool::new(true)
        ),
        Err(MdRaidPlanError::Cancelled),
    ));
    let content = hash_source(
        path.to_str().unwrap(),
        &DataSourceKind::Raw,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(content.logical_size, 0);
    assert_eq!(content.chunk_sha256, Vec::<String>::new());
    assert_eq!(
        content.logical_sha256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn e01_content_identity_and_verified_reads_use_decompressed_logical_bytes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/e01/tiny.E01");
    let content = hash_source(
        path.to_str().unwrap(),
        &DataSourceKind::E01,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut independent = image_e01::E01Reader::open(&path).unwrap();
    let expected = crate::hash_service::HashService::sha256_reader(&mut independent).unwrap();
    assert_eq!(content.logical_sha256, expected);
    assert_ne!(
        content.logical_sha256,
        crate::hash_service::HashService::sha256_file(&path).unwrap()
    );
    let source = datasource_service::open_evidence_reader(&path, &DataSourceKind::E01).unwrap();
    let mut verified = reader::ContentVerifiedReader::new(source, Arc::new(content)).unwrap();
    verified.seek(SeekFrom::End(-17)).unwrap();
    independent.seek(SeekFrom::End(-17)).unwrap();
    let mut actual = [0; 17];
    let mut expected = [0; 17];
    verified.read_exact(&mut actual).unwrap();
    independent.read_exact(&mut expected).unwrap();
    assert_eq!(actual, expected);
}
