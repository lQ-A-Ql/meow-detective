//! Fixed ranges are independent of performance timing and evidence-container hashes.
#[path = "support/import_ntfs_fixture.rs"]
mod fixture;
#[path = "../../fs-ntfs/tests/support/initialized_data.rs"]
mod initialized;
use evidence_core::{EvidenceReader, FileSystemReader, PartitionWindowReader};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom, Write};

#[test]
#[ignore = "explicit FORENSICS_IMPORT_FIXTURE logical-byte digest; outside import timing"]
fn digest_evidence_logical_bytes() {
    let path = std::path::PathBuf::from(
        std::env::var_os("FORENSICS_IMPORT_FIXTURE").expect("set FORENSICS_IMPORT_FIXTURE"),
    );
    let kind = if path
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("e01"))
    {
        domain::DataSourceKind::E01
    } else {
        domain::DataSourceKind::Raw
    };
    let factory = app_services::source_runtime::open_factory(&path, &kind).unwrap();
    let mut reader = factory.open_reader().unwrap();
    let expected = reader.info().size;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    let mut total = 0u64;
    loop {
        let count = reader.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        total += count as u64;
        hash.update(&buffer[..count]);
    }
    assert_eq!(total, expected);
    eprintln!(
        "[logical-byte-oracle] {}",
        serde_json::json!({"version":1,"view":"logicalDisk","bytes":total,"sha256":hex::encode(hash.finalize())})
    );
}

#[test]
fn segment_chunk_partition_and_derived_window_ranges_match_logical_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let synthetic = fixture::image(32);
    let compressed = temp.path().join("compressed.E01");
    fixture::write_e01(&compressed, &synthetic).unwrap();
    let mut logical = Vec::new();
    image_e01::E01Reader::open(&compressed)
        .unwrap()
        .read_to_end(&mut logical)
        .unwrap();
    assert_eq!(Sha256::digest(&logical), Sha256::digest(&synthetic));
    let bytes: Vec<u8> = (0..8192)
        .map(|i| ((i * 7919 + i / 4096) % 251) as u8)
        .collect();
    let path = temp.path().join("split.E01");
    segment(&path, Some(8192), &bytes[..4096]);
    segment(&temp.path().join("split.E02"), None, &bytes[4096..]);
    let plan = image_e01::E01Reader::open(&path)
        .unwrap()
        .open_plan()
        .unwrap();
    assert_eq!(plan.segment_count(), 2);
    for workers in [1, 2, 4] {
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let plan = &plan;
                let bytes = &bytes;
                scope.spawn(move || {
                    let mut reader = plan.open_reader().unwrap();
                    for (offset, length) in [(0, 16), (4080, 64), (4095, 2), (4096, 17), (8176, 16)]
                    {
                        reader.seek(SeekFrom::Start(offset)).unwrap();
                        let mut actual = vec![0; length];
                        reader.read_exact(&mut actual).unwrap();
                        assert_eq!(actual, bytes[offset as usize..offset as usize + length]);
                    }
                });
            }
        });
    }
    let reader: Box<dyn EvidenceReader> = Box::new(plan.open_reader().unwrap());
    let partition = PartitionWindowReader::new(reader, 4000, Some(300)).unwrap();
    let mut derived = PartitionWindowReader::new(Box::new(partition), 80, Some(64)).unwrap();
    let mut actual = Vec::new();
    derived.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, bytes[4080..4144]);
    assert_eq!(derived.read(&mut [0; 1]).unwrap(), 0);
    assert_ne!(
        Sha256::digest(std::fs::read(&path).unwrap()),
        Sha256::digest(&bytes)
    );
    std::fs::remove_file(temp.path().join("split.E02")).unwrap();
    assert!(plan.open_reader().is_err());
    assert!(plan.validate_snapshot().is_err());
}

#[test]
fn initialized_length_is_logical_zero_while_physical_residue_remains_readable() {
    let bytes = initialized::image(1024, 700, true);
    assert_eq!(
        &bytes[initialized::ATTRIBUTE_OFFSET..initialized::ATTRIBUTE_OFFSET + 4],
        &0x80u32.to_le_bytes()
    );
    let (ntfs, _temp) = initialized::filesystem(&bytes);
    let expected = initialized::expected(1024, 700);
    for (offset, length) in [(490, 80), (690, 40), (700, 100), (1000, 24), (1024, 1)] {
        assert_eq!(
            ntfs.read_file_range_by_inode(6, offset, length).unwrap(),
            expected[offset as usize..(offset as usize + length).min(1024)]
        );
    }
    let mut all = Vec::new();
    ntfs.open_file("mft:6")
        .unwrap()
        .read_to_end(&mut all)
        .unwrap();
    assert_eq!(all, expected);
    assert_eq!(&bytes[40 * 512 + 188..40 * 512 + 228], &[b'B'; 40]);
    assert_eq!(&all[700..740], &[0; 40]);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("initialized.E01");
    fixture::write_e01(&path, &bytes).unwrap();
    let ntfs =
        fs_ntfs::NtfsReader::open(Box::new(image_e01::E01Reader::open(&path).unwrap()), 0).unwrap();
    assert_eq!(
        ntfs.read_file_range_by_inode(6, 690, 40).unwrap(),
        expected[690..730]
    );
}

fn segment(path: &std::path::Path, total: Option<u64>, bytes: &[u8]) {
    fn section(kind: &str, next: u64, size: u64) -> [u8; 76] {
        let mut d = [0; 76];
        d[..kind.len()].copy_from_slice(kind.as_bytes());
        d[16..24].copy_from_slice(&next.to_le_bytes());
        d[24..32].copy_from_slice(&size.to_le_bytes());
        d
    }
    let table_offset = 13 + if total.is_some() { 112 } else { 0 };
    let done = table_offset + 76 + 32;
    let data = done + 76;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")
        .unwrap();
    if let Some(total) = total {
        let mut v = [0; 36];
        v[8..12].copy_from_slice(&8u32.to_le_bytes());
        v[12..16].copy_from_slice(&512u32.to_le_bytes());
        v[16..24].copy_from_slice(&(total / 512).to_le_bytes());
        f.write_all(&section("volume", table_offset, 112)).unwrap();
        f.write_all(&v).unwrap();
    }
    let mut t = [0; 32];
    t[..4].copy_from_slice(&1u32.to_le_bytes());
    t[8..16].copy_from_slice(&data.to_le_bytes());
    f.write_all(&section("table", done, 108)).unwrap();
    f.write_all(&t).unwrap();
    f.write_all(&section("done", 0, 0)).unwrap();
    f.write_all(bytes).unwrap();
    f.flush().unwrap();
}
