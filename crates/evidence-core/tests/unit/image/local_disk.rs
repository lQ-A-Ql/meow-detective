use super::*;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

#[test]
fn rejects_non_physical_drive_paths() {
    let error = LocalDiskReader::open(Path::new("C:/disk.raw")).expect_err("must reject");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn rejects_malformed_physical_drive_paths() {
    for value in [
        r"\\.\PhysicalDrive",
        r"\\.\PhysicalDrive-1",
        r"\\.\PhysicalDrive0x",
    ] {
        let error = LocalDiskReader::open(Path::new(value)).expect_err("must reject");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[test]
fn accepts_case_insensitive_and_slash_normalized_paths() {
    assert_eq!(
        parse_physical_drive_path(Path::new(r"\\.\physicaldrive12")),
        Some(12)
    );
    assert_eq!(
        parse_physical_drive_path(Path::new(r"\\./PhysicalDrive7")),
        Some(7)
    );
}

#[test]
fn rejects_disk_number_overflow() {
    assert_eq!(
        parse_physical_drive_path(Path::new(r"\\.\PhysicalDrive4294967296")),
        None
    );
}

#[test]
fn cached_reads_preserve_seek_semantics() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"0123456789abcdef").unwrap();
    let handle = std::fs::File::open(file.path()).unwrap();
    let mut reader = LocalDiskReader {
        file: handle,
        info: ReaderInfo {
            path: file.path().to_path_buf(),
            size: 16,
            kind: "local_disk".to_string(),
        },
        cursor: 0,
        cache: Vec::new(),
        cache_start: 0,
        cache_len: 0,
    };
    let mut first = [0u8; 4];
    reader.read_exact(&mut first).unwrap();
    assert_eq!(&first, b"0123");
    reader.seek(SeekFrom::Start(2)).unwrap();
    let mut second = [0u8; 10];
    reader.read_exact(&mut second).unwrap();
    assert_eq!(&second, b"23456789ab");
}

#[test]
#[ignore = "requires an administrator-readable physical disk"]
fn sequential_read_benchmark_reports_throughput() {
    let device = std::env::var("FORENSICS_LOCAL_DISK_BENCHMARK_DEVICE")
        .expect("set FORENSICS_LOCAL_DISK_BENCHMARK_DEVICE to \\\\.\\PhysicalDriveN");
    let bytes = std::env::var("FORENSICS_LOCAL_DISK_BENCHMARK_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(256 * 1024 * 1024);
    let mut reader = LocalDiskReader::open(Path::new(&device)).unwrap();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut remaining = bytes.min(reader.len());
    let started = Instant::now();
    while remaining > 0 {
        let chunk = remaining.min(buffer.len() as u64) as usize;
        reader.read_exact(&mut buffer[..chunk]).unwrap();
        remaining -= chunk as u64;
    }
    let elapsed = started.elapsed();
    let mib_per_second = bytes.min(reader.len()) as f64
        / (1024.0 * 1024.0)
        / elapsed.as_secs_f64().max(f64::EPSILON);
    eprintln!(
        "local_disk_benchmark bytes={} elapsed_ms={} mib_per_second={:.2}",
        bytes.min(reader.len()),
        elapsed.as_millis(),
        mib_per_second
    );
}
