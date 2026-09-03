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

// ——— open / identity ——————————————————————————————————————————————————

#[test]
fn opens_valid_raw_image() {
    let reader = open_tiny();
    assert!(!reader.is_empty(), "tiny.raw should be non-empty");
    assert_eq!(reader.info().kind, "raw");
}

#[test]
fn info_and_path_report_the_open_path() {
    let expected = tiny_raw_path();
    let reader = open_tiny();
    assert_eq!(reader.info().path, expected);
    assert_eq!(reader.path(), expected.as_path());
    assert_eq!(reader.info().kind, "raw");
}

#[test]
fn len_and_is_empty_track_file_size() {
    let reader = open_tiny();
    assert_eq!(reader.len(), 1024);
    assert!(!reader.is_empty());

    let (_dir, empty) = temp_raw(&[]);
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
}

#[test]
fn open_rejects_nonexistent_file() {
    let missing = tiny_raw_path().parent().unwrap().join("__no_such_file.raw");
    let error = RawImageReader::open(&missing).expect_err("nonexistent should error");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn open_rejects_directory() {
    let dir_path = tiny_raw_path().parent().unwrap().to_path_buf();
    // A directory must never yield a usable reader, but the rejection comes from
    // two different places by platform: Windows `File::open` already fails on a
    // directory, while on Unix it succeeds and the explicit `is_dir` check is
    // what rejects it. Assert the outcome, not the platform-specific message.
    assert!(
        RawImageReader::open(&dir_path).is_err(),
        "opening a directory as a raw image must fail"
    );
}

#[cfg(unix)]
#[test]
fn open_names_the_directory_rejection_on_unix() {
    let dir_path = tiny_raw_path().parent().unwrap().to_path_buf();
    let error = RawImageReader::open(&dir_path).expect_err("directory should error");
    assert!(
        error
            .to_string()
            .contains("cannot open directory as raw image"),
        "expected the explicit directory rejection, got: {error}"
    );
}

// ——— read ——————————————————————————————————————————————————————————————

#[test]
fn reads_first_sector() {
    let mut reader = open_tiny();
    let mut buf = [0u8; 512];
    let read = reader.read(&mut buf).expect("read should succeed");
    assert_eq!(read, 512, "should read a full sector");
    assert!(
        buf.iter().any(|&byte| byte != 0),
        "first sector should contain non-zero bytes"
    );
}

#[test]
fn reads_multiple_sectors_sequentially() {
    let data = vec![0xABu8; 8192];
    let (_dir, mut reader) = temp_raw(&data);

    let mut first = [0u8; 4096];
    assert_eq!(reader.read(&mut first).expect("read ok"), 4096);
    assert!(first.iter().all(|&byte| byte == 0xAB));

    let mut second = [0u8; 4096];
    assert_eq!(reader.read(&mut second).expect("read ok"), 4096);
    assert!(second.iter().all(|&byte| byte == 0xAB));
}

#[test]
fn read_at_eof_returns_zero_bytes() {
    let mut reader = open_tiny();
    let len = reader.len();
    reader.seek(SeekFrom::Start(len)).expect("seek to end");
    let mut buf = [0u8; 32];
    assert_eq!(reader.read(&mut buf).expect("read ok"), 0);
}

#[test]
fn read_exact_past_eof_reports_unexpected_eof() {
    let data = vec![0xCDu8; 100];
    let (_dir, mut reader) = temp_raw(&data);
    let mut buf = [0u8; 200];
    let error = reader
        .read_exact(&mut buf)
        .expect_err("read_exact past EOF should error");
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn repeated_reads_return_identical_bytes() {
    let data: Vec<u8> = (0..1024u32).map(|index| (index & 0xFF) as u8).collect();
    let (_dir, mut reader) = temp_raw(&data);

    let mut first = [0u8; 1024];
    reader.read_exact(&mut first).expect("first read");

    reader.seek(SeekFrom::Start(0)).expect("rewind");
    let mut second = [0u8; 1024];
    reader.read_exact(&mut second).expect("second read");

    assert_eq!(first, second);
    assert_eq!(&first[..], &data[..]);
}

// ——— seek ——————————————————————————————————————————————————————————————

#[test]
fn seeks_to_absolute_offset_then_reads() {
    let mut reader = open_tiny();
    assert_eq!(reader.seek(SeekFrom::Start(512)).expect("seek ok"), 512);
    let mut buf = [0u8; 512];
    assert_eq!(reader.read(&mut buf).expect("read ok"), 512);
}

#[test]
fn seek_from_current_moves_both_directions() {
    let data = vec![0u8; 2048];
    let (_dir, mut reader) = temp_raw(&data);

    reader.seek(SeekFrom::Start(512)).expect("seek start");
    assert_eq!(reader.seek(SeekFrom::Current(256)).expect("forward"), 768);
    assert_eq!(reader.seek(SeekFrom::Current(-128)).expect("backward"), 640);
}

#[test]
fn seek_from_end_addresses_the_tail() {
    let mut data = vec![0u8; 1024];
    data[1020..].copy_from_slice(b"TAIL");
    let (_dir, mut reader) = temp_raw(&data);

    assert_eq!(reader.seek(SeekFrom::End(-4)).expect("seek ok"), 1020);
    let mut tail = [0u8; 4];
    reader.read_exact(&mut tail).expect("read tail");
    assert_eq!(&tail, b"TAIL");

    assert_eq!(reader.seek(SeekFrom::End(0)).expect("seek ok"), 1024);
}

#[test]
fn seek_before_start_errors_rather_than_clamping() {
    let data = vec![0xEEu8; 512];
    let (_dir, mut reader) = temp_raw(&data);
    // Standard-library semantics: a negative resulting position is an error.
    // Silently clamping to 0 would hide a caller's offset bug on an evidence
    // read path, so this must stay an error.
    assert!(reader.seek(SeekFrom::End(-4096)).is_err());
    assert!(reader.seek(SeekFrom::Current(-1)).is_err());
}

#[test]
fn seek_past_eof_is_legal_and_recoverable() {
    let data = vec![0xEEu8; 512];
    let (_dir, mut reader) = temp_raw(&data);

    reader.seek(SeekFrom::Start(10_000)).expect("seek past EOF");
    let mut buf = [0u8; 16];
    assert_eq!(reader.read(&mut buf).expect("read ok"), 0);

    reader.seek(SeekFrom::Start(0)).expect("seek back");
    let mut recovered = [0u8; 512];
    assert_eq!(reader.read(&mut recovered).expect("read ok"), 512);
    assert!(recovered.iter().all(|&byte| byte == 0xEE));
}

// ——— try_clone —————————————————————————————————————————————————————————

#[test]
fn clone_reads_independently_of_the_original() {
    let mut data = vec![0u8; 512];
    data[0..4].copy_from_slice(b"SIGX");
    let (_dir, mut reader) = temp_raw(&data);

    let mut clone = reader.try_clone().expect("try_clone");
    let mut from_clone = [0u8; 4];
    clone.read_exact(&mut from_clone).expect("clone read");
    assert_eq!(&from_clone, b"SIGX");

    reader.seek(SeekFrom::Start(0)).expect("rewind original");
    let mut from_original = [0u8; 4];
    reader
        .read_exact(&mut from_original)
        .expect("original read");
    assert_eq!(&from_original, b"SIGX");
}

#[test]
fn multiple_clones_hold_separate_positions() {
    let mut data = vec![0u8; 512];
    data[0] = 0xA0;
    data[256] = 0xB0;
    data[511] = 0xC0;
    let (_dir, mut reader) = temp_raw(&data);

    let mut clone_a = reader.try_clone().expect("clone a");
    let mut clone_b = reader.try_clone().expect("clone b");

    let mut byte = [0u8; 1];

    reader.seek(SeekFrom::Start(0)).expect("seek reader");
    reader.read_exact(&mut byte).expect("reader read");
    assert_eq!(byte[0], 0xA0);

    clone_a.seek(SeekFrom::Start(256)).expect("seek clone a");
    clone_a.read_exact(&mut byte).expect("clone a read");
    assert_eq!(byte[0], 0xB0);

    clone_b.seek(SeekFrom::Start(511)).expect("seek clone b");
    clone_b.read_exact(&mut byte).expect("clone b read");
    assert_eq!(byte[0], 0xC0);

    assert_eq!(clone_a.path(), reader.path());
    assert_eq!(clone_b.len(), reader.len());
}

fn write_flat_vmdk(dir: &Path, extent_name: &str, data: &[u8]) -> PathBuf {
    assert!(data.len().is_multiple_of(512));
    std::fs::write(dir.join(extent_name), data).expect("write extent");
    let descriptor = format!(
        "# Disk DescriptorFile\nversion=1\nCID=12345678\nparentCID=ffffffff\ncreateType=\"monolithicFlat\"\nRW {} FLAT \"{}\" 0\n",
        data.len() / 512,
        extent_name
    );
    let path = dir.join("disk.vmdk");
    std::fs::write(&path, descriptor).expect("write descriptor");
    path
}

#[test]
fn reads_monolithic_flat_vmdk_extent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut data = vec![0u8; 1024];
    data[510..514].copy_from_slice(b"EDGE");
    let descriptor = write_flat_vmdk(dir.path(), "disk-flat.vmdk", &data);

    let mut reader = RawImageReader::open(&descriptor).expect("open VMDK");
    assert_eq!(reader.info().kind, "vmdk");
    assert_eq!(reader.len(), 1024);
    assert_eq!(reader.backing_paths().len(), 2);
    reader
        .seek(SeekFrom::Start(510))
        .expect("cross-sector seek");
    let mut actual = [0u8; 4];
    reader.read_exact(&mut actual).expect("cross-sector read");
    assert_eq!(&actual, b"EDGE");
}

#[test]
fn accepts_descriptor_assignment_whitespace() {
    let dir = tempfile::tempdir().expect("tempdir");
    let descriptor = write_flat_vmdk(dir.path(), "disk-flat.vmdk", &[0x5a; 512]);
    let text = std::fs::read_to_string(&descriptor)
        .expect("read descriptor")
        .replace("parentCID=", "parentCID = ")
        .replace("createType=", "createType = ");
    std::fs::write(&descriptor, text).expect("rewrite descriptor");

    let mut reader = RawImageReader::open(&descriptor).expect("open VMDK");
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte).expect("read extent");
    assert_eq!(byte, [0x5a]);
}

#[test]
fn rejects_truncated_vmdk_extent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let descriptor = write_flat_vmdk(dir.path(), "disk-flat.vmdk", &[0u8; 512]);
    let text = std::fs::read_to_string(&descriptor)
        .expect("read descriptor")
        .replace("RW 1 ", "RW 2 ");
    std::fs::write(&descriptor, text).expect("rewrite descriptor");

    let error = RawImageReader::open(&descriptor).expect_err("truncated extent");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn rejects_sparse_and_parented_vmdk_descriptors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sparse = dir.path().join("sparse.vmdk");
    std::fs::write(
        &sparse,
        "# Disk DescriptorFile\nparentCID=ffffffff\ncreateType=\"streamOptimized\"\nRW 1 SPARSE \"disk-sparse.vmdk\"\n",
    )
    .expect("write sparse descriptor");
    assert_eq!(
        RawImageReader::open(&sparse)
            .expect_err("sparse unsupported")
            .kind(),
        io::ErrorKind::Unsupported
    );

    let parented = write_flat_vmdk(dir.path(), "disk-flat.vmdk", &[0u8; 512]);
    let text = std::fs::read_to_string(&parented)
        .expect("read descriptor")
        .replace("parentCID=ffffffff", "parentCID=12345678");
    std::fs::write(&parented, text).expect("rewrite parent descriptor");
    assert_eq!(
        RawImageReader::open(&parented)
            .expect_err("parent chain unsupported")
            .kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn rejects_binary_sparse_vmdk_magic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sparse = dir.path().join("binary-sparse.vmdk");
    let mut bytes = vec![0u8; 2 * 1024 * 1024];
    bytes[..4].copy_from_slice(b"KDMV");
    std::fs::write(&sparse, bytes).expect("write sparse image");
    assert_eq!(
        RawImageReader::open(&sparse)
            .expect_err("binary sparse unsupported")
            .kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn rejects_unrecognized_vmdk_by_extension_instead_of_raw_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("unknown.vmdk");
    std::fs::write(&path, [0u8; 512]).expect("write unknown VMDK");

    assert_eq!(
        RawImageReader::open(&path)
            .expect_err("unknown VMDK must fail closed")
            .kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn rejects_split_raw_sets_instead_of_parsing_one_member() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("capture.001");
    let second = dir.path().join("capture.002");
    std::fs::write(&first, [0u8; 512]).expect("write first member");
    std::fs::write(&second, [0u8; 512]).expect("write second member");

    for member in [&first, &second] {
        assert_eq!(
            RawImageReader::open(member)
                .expect_err("split RAW is unsupported")
                .kind(),
            io::ErrorKind::Unsupported
        );
    }
}

#[test]
fn rejects_vmdk_extent_path_escape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let descriptor = dir.path().join("escape.vmdk");
    std::fs::write(
        &descriptor,
        "# Disk DescriptorFile\nparentCID=ffffffff\ncreateType=\"monolithicFlat\"\nRW 1 FLAT \"../outside.raw\" 0\n",
    )
    .expect("write descriptor");
    let error = RawImageReader::open(&descriptor).expect_err("path escape");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn rejects_mixed_and_self_referencing_vmdk_extents() {
    let dir = tempfile::tempdir().expect("tempdir");
    let descriptor = write_flat_vmdk(dir.path(), "disk-flat.vmdk", &[0u8; 512]);
    let mut text = std::fs::read_to_string(&descriptor).expect("read descriptor");
    text.push_str("RDONLY 1 FLAT \"disk-flat.vmdk\" 0\n");
    std::fs::write(&descriptor, text).expect("rewrite descriptor");
    assert_eq!(
        RawImageReader::open(&descriptor)
            .expect_err("mixed extents")
            .kind(),
        io::ErrorKind::Unsupported
    );

    let self_reference = dir.path().join("self.vmdk");
    std::fs::write(
        &self_reference,
        "# Disk DescriptorFile\nparentCID=ffffffff\ncreateType=\"monolithicFlat\"\nRW 1 FLAT \"self.vmdk\" 0\n",
    )
    .expect("write self-reference descriptor");
    assert_eq!(
        RawImageReader::open(&self_reference)
            .expect_err("self-reference")
            .kind(),
        io::ErrorKind::InvalidData
    );
}
