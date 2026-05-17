use evidence_core::{probe, EvidenceReader, RawImageReader};
use std::io::Read;
use tempfile::TempDir;

#[test]
fn open_and_read_first_sector() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("test.img");
    let data = vec![0xEBu8; 512];
    std::fs::write(&path, &data).unwrap();

    let mut reader = RawImageReader::open(&path).unwrap();
    assert_eq!(reader.info().size, 512);
    assert_eq!(reader.info().kind, "raw");

    let mut buf = [0u8; 512];
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf[0], 0xEB);
}

#[test]
fn seek_and_read() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("seek.img");
    let mut data = vec![0u8; 1024];
    data[512] = 0x42;
    std::fs::write(&path, &data).unwrap();

    let mut reader = RawImageReader::open(&path).unwrap();
    use std::io::Seek;
    reader.seek(std::io::SeekFrom::Start(512)).unwrap();
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf[0], 0x42);
}

#[test]
fn probe_identifies_raw() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("image.dd");
    std::fs::write(&path, b"placeholder").unwrap();
    let result = probe::probe(&path).unwrap();
    assert!(result.candidates.contains(&"raw".to_string()));
}

#[test]
fn probe_identifies_logical_directory() {
    let tmp = TempDir::new().unwrap();
    let result = probe::probe(tmp.path()).unwrap();
    assert!(result.candidates.contains(&"logical_directory".to_string()));
}

#[test]
fn probe_rejects_nonexistent() {
    let result = probe::probe(std::path::Path::new("/nonexistent/path"));
    assert!(result.is_err());
}
