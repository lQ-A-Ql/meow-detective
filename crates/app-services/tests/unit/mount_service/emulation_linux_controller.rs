use std::fmt::Write as _;
use std::io::Write;

use evidence_emulation::VmdkAdapter;

use super::super::emulation_linux_controller::{
    inspect_initramfs_driver_names, LinuxControllerEvidence,
};

fn cpio_archive(names: &[&str]) -> Vec<u8> {
    let mut archive = Vec::new();
    for name in names {
        append_cpio_entry(&mut archive, name, 0o100644);
    }
    append_cpio_entry(&mut archive, "TRAILER!!!", 0);
    archive
}

fn append_cpio_entry(archive: &mut Vec<u8>, name: &str, mode: u32) {
    let name_bytes = name.as_bytes();
    let mut header = String::new();
    write!(
        header,
        "070701{ino:08x}{mode:08x}{uid:08x}{gid:08x}{nlink:08x}{mtime:08x}{filesize:08x}{devmajor:08x}{devminor:08x}{rdevmajor:08x}{rdevminor:08x}{namesize:08x}{check:08x}",
        ino = 0,
        mode = mode,
        uid = 0,
        gid = 0,
        nlink = 1,
        mtime = 0,
        filesize = 0,
        devmajor = 0,
        devminor = 0,
        rdevmajor = 0,
        rdevminor = 0,
        namesize = name_bytes.len() + 1,
        check = 0,
    )
    .unwrap();
    archive.extend_from_slice(header.as_bytes());
    archive.extend_from_slice(name_bytes);
    archive.push(0);
    pad4(archive);
}

fn pad4(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
}

fn evidence_from_archive(archive: &[u8]) -> LinuxControllerEvidence {
    let (ide, lsi) = inspect_initramfs_driver_names(archive).expect("valid initramfs");
    LinuxControllerEvidence {
        ide,
        lsi,
        found_initramfs: true,
        decoded_initramfs: true,
    }
}

#[test]
fn selects_ide_for_ata_piix() {
    assert_eq!(
        evidence_from_archive(&cpio_archive(&[
            "lib/modules/3.10/kernel/drivers/ata/ATA_PIIX.KO.XZ",
        ]))
        .decision()
        .adapter,
        VmdkAdapter::Ide
    );
}

#[test]
fn selects_lsi_when_mptspi_is_present() {
    let evidence = evidence_from_archive(&cpio_archive(&[
        "usr/lib/modules/6.6/kernel/drivers/scsi/mptbase.ko",
        "usr/lib/modules/6.6/kernel/drivers/scsi/mptscsih.ko.zst",
        "usr/lib/modules/6.6/kernel/drivers/scsi/mptspi.ko",
        "usr/lib/modules/6.6/kernel/drivers/scsi/vmw_pvscsi.ko",
    ]));
    assert!(!evidence.ide);
    assert!(evidence.lsi);
    assert_eq!(evidence.decision().adapter, VmdkAdapter::LsiLogic);
}

#[test]
fn pvscsi_without_mptspi_does_not_select_lsi_logic() {
    let evidence = evidence_from_archive(&cpio_archive(&[
        "usr/lib/modules/6.6/kernel/drivers/scsi/vmw_pvscsi.ko",
    ]));
    assert!(!evidence.lsi);
    assert_eq!(evidence.decision().adapter, VmdkAdapter::Ide);
    assert!(evidence
        .decision()
        .reason
        .contains("no supported initramfs storage driver"));
}

#[test]
fn backup_and_disabled_module_names_do_not_select_a_controller() {
    let evidence = evidence_from_archive(&cpio_archive(&[
        "kernel/drivers/ata/ata_piix.ko.disabled",
        "kernel/drivers/scsi/mptspi.ko.backup",
    ]));
    assert!(!evidence.ide);
    assert!(!evidence.lsi);
    assert!(evidence
        .decision()
        .reason
        .contains("no supported initramfs storage driver"));
}

#[test]
fn ide_wins_when_initramfs_contains_both_controller_families() {
    let evidence = evidence_from_archive(&cpio_archive(&[
        "kernel/drivers/ata/ata_piix.ko",
        "kernel/drivers/scsi/mptspi.ko",
    ]));
    assert_eq!(evidence.decision().adapter, VmdkAdapter::Ide);
}

#[test]
fn decodes_gzip_xz_and_zstd_cpio() {
    let archive = cpio_archive(&["kernel/drivers/scsi/mptspi.ko"]);

    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(&archive).unwrap();
    let gzip = gzip.finish().unwrap();
    assert!(inspect_initramfs_driver_names(&gzip).unwrap().1);

    let mut xz = xz2::write::XzEncoder::new(Vec::new(), 6);
    xz.write_all(&archive).unwrap();
    let xz = xz.finish().unwrap();
    assert!(inspect_initramfs_driver_names(&xz).unwrap().1);

    let zstd = zstd::stream::encode_all(std::io::Cursor::new(&archive), 1).unwrap();
    assert!(inspect_initramfs_driver_names(&zstd).unwrap().1);
}

#[test]
fn scans_a_compressed_main_archive_after_early_microcode_cpio() {
    let mut image = cpio_archive(&["kernel/x86/microcode/GenuineIntel.bin"]);
    image.extend_from_slice(&[0; 16]);
    let main = cpio_archive(&["kernel/drivers/ata/ata_piix.ko"]);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(&main).unwrap();
    image.extend_from_slice(&gzip.finish().unwrap());

    let (ide, lsi) = inspect_initramfs_driver_names(&image).expect("concatenated initramfs");
    assert!(ide);
    assert!(!lsi);
}

#[test]
fn malformed_or_unknown_archive_defaults_to_ide() {
    assert!(inspect_initramfs_driver_names(b"not-an-initramfs").is_none());
    let decision = LinuxControllerEvidence {
        found_initramfs: true,
        ..LinuxControllerEvidence::default()
    }
    .decision();
    assert_eq!(decision.adapter, VmdkAdapter::Ide);
    assert!(decision.reason.contains("could not be decoded"));
}

#[test]
fn truncated_cpio_keeps_evidence_from_parsed_entries() {
    let mut archive = cpio_archive(&["kernel/drivers/ata/ata_piix.ko"]);
    let trailer_len = cpio_archive(&[]).len();
    archive.truncate(archive.len() - trailer_len);
    let (ide, lsi) = inspect_initramfs_driver_names(&archive)
        .expect("fully parsed entries survive a missing trailer");
    assert!(ide);
    assert!(!lsi);
}

#[test]
fn truncated_trailer_padding_still_decodes_the_archive() {
    let mut archive = cpio_archive(&["kernel/drivers/ata/ata_piix.ko"]);
    archive.pop();
    let (ide, _) =
        inspect_initramfs_driver_names(&archive).expect("trailer padding may be cut off");
    assert!(ide);
}

#[test]
fn decodes_lz4_cpio() {
    let archive = cpio_archive(&["kernel/drivers/scsi/mptspi.ko.lz4"]);
    let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
    encoder.write_all(&archive).unwrap();
    let lz4 = encoder.finish().unwrap();
    assert!(inspect_initramfs_driver_names(&lz4).unwrap().1);
}

#[test]
fn skips_a_corrupt_segment_and_keeps_evidence_from_the_others() {
    let mut image = cpio_archive(&["kernel/drivers/ata/ata_piix.ko"]);
    image.extend_from_slice(b"070701");
    image.extend_from_slice(&[b'Z'; 200]);
    let mut main = cpio_archive(&["kernel/drivers/scsi/mptspi.ko"]);
    image.append(&mut main);

    let (ide, lsi) = inspect_initramfs_driver_names(&image)
        .expect("a corrupt segment must not poison the others");
    assert!(ide);
    assert!(lsi);
}

#[test]
fn keeps_earlier_segment_evidence_when_the_compressed_part_is_undecodable() {
    let mut image = cpio_archive(&["kernel/drivers/ata/ata_piix.ko"]);
    image.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00, 0xde, 0xad, 0xbe, 0xef]);

    let (ide, lsi) = inspect_initramfs_driver_names(&image)
        .expect("an undecodable tail must not discard earlier evidence");
    assert!(ide);
    assert!(!lsi);
}

fn gzip_segment_wrapping(payload: &[u8]) -> Vec<u8> {
    let mut segment = cpio_archive(&[]);
    segment.extend_from_slice(payload);
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&segment).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn decodes_nested_compressed_segments_up_to_the_depth_bound() {
    let mut nested = cpio_archive(&["kernel/drivers/scsi/mptspi.ko"]);
    for _ in 0..4 {
        nested = gzip_segment_wrapping(&nested);
    }
    let (_, lsi) = inspect_initramfs_driver_names(&nested).expect("within the depth bound");
    assert!(lsi);
}

#[test]
fn ignores_segments_nested_beyond_the_depth_bound() {
    let mut nested = cpio_archive(&["kernel/drivers/scsi/mptspi.ko"]);
    for _ in 0..5 {
        nested = gzip_segment_wrapping(&nested);
    }
    let (ide, lsi) =
        inspect_initramfs_driver_names(&nested).expect("the outer archives still decode");
    assert!(!ide);
    assert!(!lsi);
}
