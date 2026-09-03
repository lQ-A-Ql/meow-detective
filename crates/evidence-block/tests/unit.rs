use std::io::Write;

use evidence_block::{EvidenceImageKind, ReadOnlyScsiDevice};
use iscsi_target::scsi::{asc, scsi_status, ScsiHandler};
use iscsi_target::ScsiBlockDevice;

fn raw_device() -> (tempfile::NamedTempFile, ReadOnlyScsiDevice) {
    let mut file = tempfile::NamedTempFile::new().expect("temp image");
    let mut bytes = vec![0u8; 4096];
    bytes[512..1024].fill(0x5a);
    file.write_all(&bytes).expect("write image");
    file.flush().expect("flush image");
    let device = ReadOnlyScsiDevice::open(file.path(), EvidenceImageKind::Raw).expect("device");
    (file, device)
}

#[test]
fn reads_aligned_blocks_from_a_real_raw_file() {
    let (_file, device) = raw_device();
    let data = device.read(1, 1, 512).expect("read block");
    assert_eq!(data, vec![0x5a; 512]);
}

#[test]
fn reads_blocks_through_a_monolithic_flat_vmdk_descriptor() {
    let directory = tempfile::tempdir().expect("tempdir");
    let extent = directory.path().join("disk-flat.vmdk");
    let descriptor = directory.path().join("disk.vmdk");
    let mut bytes = vec![0u8; 1024];
    bytes[512..].fill(0xa5);
    std::fs::write(&extent, bytes).expect("write extent");
    std::fs::write(
        &descriptor,
        "# Disk DescriptorFile\nparentCID=ffffffff\ncreateType=\"monolithicFlat\"\nRW 2 FLAT \"disk-flat.vmdk\" 0\n",
    )
    .expect("write descriptor");

    let device = ReadOnlyScsiDevice::open(&descriptor, EvidenceImageKind::Raw).expect("open VMDK");
    assert_eq!(device.read(1, 1, 512).expect("read block"), vec![0xa5; 512]);
}

#[test]
fn rejects_write_commands_with_data_protect() {
    let (_file, device) = raw_device();
    let cdb = [0x2a, 0, 0, 0, 0, 0, 0, 0, 1, 0];
    let response =
        ScsiHandler::handle_command(&cdb, &device, Some(&vec![0u8; 512])).expect("SCSI response");
    assert_eq!(response.status, scsi_status::CHECK_CONDITION);
    assert_eq!(response.sense.expect("sense").asc, asc::WRITE_PROTECTED);
}

#[test]
fn mode_sense_reports_write_protection() {
    let (_file, device) = raw_device();
    let response =
        ScsiHandler::handle_command(&[0x1a, 0, 0x3f, 0, 4, 0], &device, None).expect("mode sense");
    assert_eq!(response.data[2] & 0x80, 0x80);
}

#[test]
fn rejects_unaligned_images() {
    let mut file = tempfile::NamedTempFile::new().expect("temp image");
    file.write_all(&[0u8; 513]).expect("write image");
    let result = ReadOnlyScsiDevice::open(file.path(), EvidenceImageKind::Raw);
    assert!(result.is_err());
}
