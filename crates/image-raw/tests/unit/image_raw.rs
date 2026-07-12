use super::*;
use std::path::PathBuf;

// ——— helpers ——————————————————————————————————————————————————————————

/// Absolute path to the tiny raw fixture (checked into the repo).
fn tiny_raw_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("testdata")
        .join("fixtures")
        .join("public-small")
        .join("raw")
        .join("tiny.raw")
}

/// Helper: open the tiny raw fixture and return the reader.
fn open_tiny() -> RawImageReader {
    RawImageReader::open(&tiny_raw_path()).expect("should open tiny.raw")
}

/// Helper: create a temp file with known content and open it.
fn temp_raw(data: &[u8]) -> (tempfile::TempDir, RawImageReader) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("temp.raw");
    std::fs::write(&path, data).expect("write temp file");
    let reader = RawImageReader::open(&path).expect("open temp file");
    (dir, reader)
}

// ——— 1. open_valid_raw_image —————————————————————————————————————————

#[test]
fn test_open_valid_raw_image() {
    let reader = open_tiny();
    assert!(!reader.is_empty(), "tiny.raw should be non-empty");
    assert_eq!(reader.info().kind, "raw");
    let _ = reader; // ensure Drop works
}

// ——— 2. read_sector_zero ——————————————————————————————————————————————

#[test]
fn test_read_sector_zero() {
    let mut reader = open_tiny();
    let mut buf = [0u8; 512];
    let n = reader.read(&mut buf).expect("read should succeed");
    assert_eq!(n, 512, "should read a full sector");
    // The buffer must not be all zeros — even minimal fixture data is non-null.
    let non_zero = buf.iter().any(|&b| b != 0);
    assert!(non_zero, "first sector should contain non-zero bytes");
}

// ——— 3. seek_and_read —————————————————————————————————————————————————

#[test]
fn test_seek_and_read() {
    let mut reader = open_tiny();

    // tiny.raw is 1024 bytes; seeking to 4096 is past EOF — that's fine,
    // we just validate we can seek+read (read should return 0 bytes).
    let pos = reader.seek(SeekFrom::Start(4096)).expect("seek ok");
    assert_eq!(pos, 4096);

    let mut buf = [0u8; 512];
    let n = reader.read(&mut buf).expect("read ok");
    // Past EOF, read returns 0 bytes.
    assert_eq!(n, 0);

    // Now seek to a valid location (offset 512, second sector).
    let pos = reader.seek(SeekFrom::Start(512)).expect("seek ok");
    assert_eq!(pos, 512);

    let mut buf = [0u8; 512];
    let n = reader.read(&mut buf).expect("read ok");
    assert_eq!(n, 512);
}

// ——— 4. read_beyond_eof ———————————————————————————————————————————————

#[test]
fn test_read_beyond_eof() {
    let mut reader = open_tiny();
    let len = reader.len();
    // Seek to the very end.
    reader.seek(SeekFrom::Start(len)).expect("seek to end");
    let mut buf = [0u8; 32];
    let n = reader.read(&mut buf).expect("read ok");
    assert_eq!(n, 0, "read at EOF should return 0 bytes, got {}", n);
}

// ——— 5. info_returns_path —————————————————————————————————————————————

#[test]
fn test_info_returns_path() {
    let expected = tiny_raw_path();
    let reader = open_tiny();
    assert_eq!(reader.info().path, expected);
    assert_eq!(reader.path(), expected.as_path());
    assert_eq!(reader.info().kind, "raw");
}

// ——— 6. open_nonexistent_file —————————————————————————————————————————

#[test]
fn test_open_nonexistent_file() {
    let missing = tiny_raw_path().parent().unwrap().join("__no_such_file.raw");
    let result = RawImageReader::open(&missing);
    assert!(result.is_err(), "opening nonexistent file should error");
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

// ——— 7. open_directory ————————————————————————————————————————————————

#[test]
fn test_open_directory() {
    let dir_path = tiny_raw_path().parent().unwrap().to_path_buf();
    let result = RawImageReader::open(&dir_path);
    assert!(result.is_err(), "opening directory should error");
}

// ——— 8. read_multiple_sectors ——————————————————————————————————————————

#[test]
fn test_read_multiple_sectors() {
    // Use a temp file large enough for multiple sector-sized reads.
    let data = vec![0xABu8; 8192]; // 16 sectors
    let (_dir, mut reader) = temp_raw(&data);

    // Read 8 sectors (4096 bytes) starting from offset 0.
    let mut buf = [0u8; 4096];
    let n = reader.read(&mut buf).expect("read ok");
    assert_eq!(n, 4096);
    assert!(buf.iter().all(|&b| b == 0xAB), "all bytes should be 0xAB");

    // Read another 4096 — should get the rest.
    let mut buf2 = [0u8; 4096];
    let n2 = reader.read(&mut buf2).expect("read ok");
    assert_eq!(n2, 4096);
    assert!(buf2.iter().all(|&b| b == 0xAB));
}

// ——— 9. seek_from_end —————————————————————————————————————————————————

#[test]
fn test_seek_from_end() {
    // File of exactly 1024 bytes with known tail marker.
    let mut data = vec![0u8; 1024];
    data[1020..].copy_from_slice(b"TAIL");
    let (_dir, mut reader) = temp_raw(&data);

    // SeekFrom::End(-4) → last 4 bytes.
    let pos = reader.seek(SeekFrom::End(-4)).expect("seek ok");
    assert_eq!(pos, 1020);

    let mut tail = [0u8; 4];
    reader.read_exact(&mut tail).expect("read tail");
    assert_eq!(&tail, b"TAIL");

    // SeekFrom::End(0) → EOF position.
    let pos = reader.seek(SeekFrom::End(0)).expect("seek ok");
    assert_eq!(pos, 1024);
}

// ——— 10. clone_and_read ————————————————————————————————————————————————

#[test]
fn test_clone_and_read() {
    let mut data = vec![0u8; 512];
    data[0..4].copy_from_slice(b"SIGX");
    let (_dir, mut reader) = temp_raw(&data);

    let mut clone = reader.clone();

    // Read from the clone first (both share a duplicate file handle which
    // may or may not share the seek position depending on platform).
    let mut buf = [0u8; 4];
    clone.read_exact(&mut buf).expect("clone read");
    assert_eq!(&buf, b"SIGX", "clone should read the signature at offset 0");

    // After clone consumed 4 bytes, seek the original back to 0 and verify
    // it can also read.
    reader.seek(SeekFrom::Start(0)).unwrap();
    let mut buf2 = [0u8; 4];
    reader.read_exact(&mut buf2).expect("orig read after seek");
    assert_eq!(&buf2, b"SIGX", "original should also read signature");
}

// ——— 11. seek_current —————————————————————————————————————————————————

#[test]
fn test_seek_current() {
    let data = vec![0u8; 2048];
    let (_dir, mut reader) = temp_raw(&data);

    // Seek forward 512 from start.
    reader.seek(SeekFrom::Start(512)).unwrap();
    // Seek forward another 256 via SeekFrom::Current.
    let pos = reader.seek(SeekFrom::Current(256)).unwrap();
    assert_eq!(pos, 768);

    // Seek backward 128.
    let pos = reader.seek(SeekFrom::Current(-128)).unwrap();
    assert_eq!(pos, 640);
}

// ——— 12. read_exact_partial_read ———————————————————————————————————————

#[test]
fn test_read_exact_partial_read() {
    let data = vec![0xCDu8; 100];
    let (_dir, mut reader) = temp_raw(&data);

    // read_exact for more than available → should error with UnexpectedEof.
    let mut buf = [0u8; 200];
    let result = reader.read_exact(&mut buf);
    assert!(result.is_err(), "read_exact past EOF should error");
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
}

// ——— 13. len_and_is_empty ——————————————————————————————————————————————

#[test]
fn test_len_and_is_empty() {
    let reader = open_tiny();
    assert_eq!(reader.len(), 1024);
    assert!(!reader.is_empty());

    // Empty file.
    let (_dir, reader2) = temp_raw(&[]);
    assert_eq!(reader2.len(), 0);
    assert!(reader2.is_empty());
}

// ——— 14. seek_past_eof_then_seek_back ——————————————————————————————————

#[test]
fn test_seek_past_eof_then_seek_back() {
    let data = vec![0xEEu8; 512];
    let (_dir, mut reader) = temp_raw(&data);

    // Seek past end.
    reader.seek(SeekFrom::Start(10_000)).unwrap();
    let mut buf = [0u8; 16];
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 0, "read past EOF should return 0");

    // Seek back to a valid position.
    reader.seek(SeekFrom::Start(0)).unwrap();
    let mut buf2 = [0u8; 512];
    let n2 = reader.read(&mut buf2).unwrap();
    assert_eq!(n2, 512);
    assert!(buf2.iter().all(|&b| b == 0xEE));
}

// ——— 15. multiple_clones_can_read ——————————————————————————————————————

#[test]
fn test_multiple_clones_can_read() {
    let mut data = vec![0u8; 512];
    data[0] = 0xA0;
    data[256] = 0xB0;
    data[511] = 0xC0;
    let (_dir, mut reader) = temp_raw(&data);

    let mut clone_a = reader.clone();
    let mut clone_b = reader.clone();

    // All three handles can read from the file (they may share a file
    // pointer, but seeking + reading on each works).
    reader.seek(SeekFrom::Start(0)).unwrap();
    let mut b = [0u8];
    reader.read_exact(&mut b).unwrap();
    assert_eq!(b[0], 0xA0, "reader at offset 0");

    clone_a.seek(SeekFrom::Start(256)).unwrap();
    clone_a.read_exact(&mut b).unwrap();
    assert_eq!(b[0], 0xB0, "clone_a at offset 256");

    clone_b.seek(SeekFrom::Start(511)).unwrap();
    clone_b.read_exact(&mut b).unwrap();
    assert_eq!(b[0], 0xC0, "clone_b at offset 511");
}

// ——— 16. reads_return_consistent_data ——————————————————————————————————

#[test]
fn test_reads_return_consistent_data() {
    // Write a predictable pattern and verify re-reads are stable.
    let data: Vec<u8> = (0..1024u32).map(|i| (i & 0xFF) as u8).collect();
    let (_dir, mut reader) = temp_raw(&data);

    let mut buf1 = [0u8; 1024];
    reader.read_exact(&mut buf1).unwrap();

    reader.seek(SeekFrom::Start(0)).unwrap();
    let mut buf2 = [0u8; 1024];
    reader.read_exact(&mut buf2).unwrap();

    assert_eq!(buf1, buf2);
    assert_eq!(&buf1[..], &data[..]);
}
