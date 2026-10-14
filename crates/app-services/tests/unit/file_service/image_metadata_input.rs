use super::{MetadataInputError, MetadataReader};

#[test]
fn read_quota_is_enforced_before_calling_the_source() {
    let mut calls = 0;
    let mut reader = MetadataReader::new(u64::MAX, |_, length| {
        calls += 1;
        Ok(vec![0; length as usize])
    });
    assert!(matches!(
        reader.bytes(0, super::MAX_METADATA_BYTES + 1),
        Err(MetadataInputError::Limit)
    ));
    drop(reader);
    assert_eq!(calls, 0);
}

#[test]
fn overflowing_ranges_and_short_reads_are_corrupt() {
    let mut reader = MetadataReader::new(u64::MAX, |_, _| Ok(vec![]));
    assert!(matches!(
        reader.bytes(u64::MAX, 1),
        Err(MetadataInputError::Corrupt)
    ));
    assert!(matches!(
        reader.bytes(0, 1),
        Err(MetadataInputError::Corrupt)
    ));
}

#[test]
fn corrupt_jpeg_segment_does_not_scan_into_image_payload() {
    let bytes = [0xff, 0xd8, 0xff, 0xe1, 0, 1];
    let mut reader = MetadataReader::new(bytes.len() as u64, |offset, length| {
        Ok(bytes[offset as usize..offset as usize + length as usize].to_vec())
    });
    assert!(matches!(
        reader.container(),
        Err(MetadataInputError::Corrupt)
    ));
}
