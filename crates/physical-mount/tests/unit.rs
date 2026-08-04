use std::io::Write;

use evidence_block::ReadOnlyScsiDevice;
use iscsi_target::ScsiBlockDevice;

#[test]
fn physical_source_device_is_sector_aligned_and_read_only() {
    let mut image = tempfile::NamedTempFile::new().expect("temp image");
    image.write_all(&vec![0x42; 4096]).expect("write image");
    image.flush().expect("flush image");
    let device = ReadOnlyScsiDevice::open(image.path(), evidence_block::EvidenceImageKind::Raw)
        .expect("open device");
    assert_eq!(device.capacity(), 8);
    assert!(device.is_read_only());
}
