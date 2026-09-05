use evidence_core::{EvidenceReader, FileSystemReader, Iso9660Reader, RawImageReader};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/public-medium")
}

#[test]
fn committed_medium_iso_exposes_joliet_tree_and_nested_reads() {
    let path = fixture_root().join("iso/medium.iso");
    let filesystem = Iso9660Reader::open(&path).expect("open committed ISO fixture");
    let root = filesystem.list_children("").expect("list ISO root");

    assert_eq!(
        root.iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["REPORT", "README.TXT", "报告.TXT"]
    );

    let report = filesystem
        .list_children("REPORT")
        .expect("list nested report directory");
    assert_eq!(
        report
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["DATA", "SUMMARY.TXT"]
    );

    let mut file = filesystem
        .open_file_seekable("REPORT/DATA/VALUES.BIN")
        .expect("open nested binary file");
    file.seek(SeekFrom::Start(1020)).expect("seek nested file");
    let mut bytes = [0u8; 8];
    file.read_exact(&mut bytes).expect("read nested file");
    assert_eq!(bytes, [252, 253, 254, 255, 0, 1, 2, 3]);
}

#[test]
fn committed_flat_vmdk_preserves_iso_bytes_and_backing_manifest() {
    let path = fixture_root().join("vmdk/medium-flat.vmdk");
    let mut reader = RawImageReader::open(&path).expect("open committed VMDK fixture");

    assert_eq!(reader.info().kind, "vmdk");
    assert_eq!(reader.len(), 524_288);
    assert_eq!(reader.backing_paths().len(), 2);
    let mut header = [0u8; 6];
    reader
        .read_exact(&mut header)
        .expect("read VMDK logical bytes");
    assert_eq!(&header, b"\0\0\0\0\0\0");

    reader
        .seek(SeekFrom::Start(16 * 2048 + 1))
        .expect("seek to ISO descriptor");
    let mut signature = [0u8; 5];
    reader
        .read_exact(&mut signature)
        .expect("read ISO descriptor signature");
    assert_eq!(&signature, b"CD001");
}

#[test]
fn committed_vmdk_can_be_composed_with_iso_reader() {
    let path = fixture_root().join("vmdk/medium-flat.vmdk");
    let reader = RawImageReader::open(&path).expect("open VMDK fixture");
    let filesystem = Iso9660Reader::from_reader(Box::new(reader), Some("medium-flat.vmdk"))
        .expect("open ISO over VMDK");
    let root = filesystem
        .list_children("")
        .expect("list ISO over VMDK root");
    assert!(root.iter().any(|entry| entry.name == "报告.TXT"));
}
