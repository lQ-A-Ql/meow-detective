use fs_ntfs::NtfsReader;
#[path = "mft_metadata.rs"]
pub(crate) mod mft_metadata;

pub const RECORD_OFFSET: usize = 7168;
pub const ATTRIBUTE_OFFSET: usize = RECORD_OFFSET + 56;
pub const DATA_OFFSET: usize = 32 * 512;

pub fn image(real_size: u64, initialized_size: u64, split: bool) -> Vec<u8> {
    let mut image = vec![0u8; 41 * 512];
    image[3..11].copy_from_slice(b"NTFS    ");
    image[11..13].copy_from_slice(&512u16.to_le_bytes());
    image[13] = 1;
    image[48..56].copy_from_slice(&2u64.to_le_bytes());
    image[64] = (-10i8) as u8;
    let record = &mut image[RECORD_OFFSET..RECORD_OFFSET + 1024];
    record[..4].copy_from_slice(b"FILE");
    record[20..22].copy_from_slice(&56u16.to_le_bytes());
    record[22..24].copy_from_slice(&1u16.to_le_bytes());
    let end = nonresident(record, 56, 0, if split { 1 } else { 2 }, 32);
    record[56 + 40..56 + 48].copy_from_slice(&1024u64.to_le_bytes());
    record[56 + 48..56 + 56].copy_from_slice(&real_size.to_le_bytes());
    record[56 + 56..56 + 64].copy_from_slice(&initialized_size.to_le_bytes());
    let end = if split {
        // Continuation size fields are intentionally zero, as on real NTFS extents.
        nonresident(record, end, 1, 1, 40)
    } else {
        end
    };
    record[end..end + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    image[DATA_OFFSET..DATA_OFFSET + 512].fill(b'A');
    image[DATA_OFFSET + 512..DATA_OFFSET + 1024].fill(b'B');
    image[40 * 512..41 * 512].fill(b'B');
    mft_metadata::initialize(&mut image);
    image
}

fn nonresident(record: &mut [u8], position: usize, vcn: u64, clusters: u8, lcn: u8) -> usize {
    record[position..position + 4].copy_from_slice(&0x80u32.to_le_bytes());
    record[position + 4..position + 8].copy_from_slice(&72u32.to_le_bytes());
    record[position + 8] = 1;
    record[position + 16..position + 24].copy_from_slice(&vcn.to_le_bytes());
    record[position + 24..position + 32]
        .copy_from_slice(&(vcn + u64::from(clusters) - 1).to_le_bytes());
    record[position + 32..position + 34].copy_from_slice(&64u16.to_le_bytes());
    record[position + 64..position + 68].copy_from_slice(&[0x11, clusters, lcn, 0]);
    position + 72
}

pub fn filesystem(image: &[u8]) -> (NtfsReader, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("initialized-data.raw");
    std::fs::write(&path, image).unwrap();
    let reader = NtfsReader::open(
        Box::new(evidence_core::RawImageReader::open(&path).unwrap()),
        0,
    )
    .unwrap();
    (reader, directory)
}

pub fn expected(real_size: usize, initialized_size: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; real_size];
    for (offset, byte) in bytes.iter_mut().take(initialized_size).enumerate() {
        *byte = if offset < 512 { b'A' } else { b'B' };
    }
    bytes
}
