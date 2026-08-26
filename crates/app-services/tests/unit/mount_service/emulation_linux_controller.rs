use std::fmt::Write as _;
use std::io::Write;

use evidence_emulation::VmdkAdapter;

use super::super::emulation_linux_controller::{
    decode_initramfs, inspect_initramfs_driver_names, parse_cpio_archive, LinuxControllerEvidence,
};

fn cpio_archive(names: &[&str]) -> Vec<u8> {
    let mut archive = Vec::new();
    for name in names {
        let name_bytes = name.as_bytes();
        let namesize = name_bytes.len() + 1;
        let mut header = String::new();
        write!(
            header,
            "070701{ino:08x}{mode:08x}{uid:08x}{gid:08x}{nlink:08x}{mtime:08x}{filesize:08x}{devmajor:08x}{devminor:08x}{rdevmajor:08x}{rdevminor:08x}{namesize:08x}{check:08x}",
            ino = 1,
            mode = 0o100644,
            uid = 0,
            gid = 0,
            nlink = 1,
            mtime = 0,
            filesize = 0,
            devmajor = 0,
            devminor = 0,
            rdevmajor = 0,
            rdevminor = 0,
            namesize = namesize,
            check = 0,
        )
        .unwrap();
        archive.extend_from_slice(header.as_bytes());
        archive.extend_from_slice(name_bytes);
        archive.push(0);
        pad4(&mut archive);
    }
    let trailer = cpio_archive_entry("TRAILER!!!");
    archive.extend_from_slice(&trailer);
    archive
}

fn cpio_archive_entry(name: &str) -> Vec<u8> {
    let name_bytes = name.as_bytes();
    let mut header = String::new();
    write!(
        header,
        "070701{ino:08x}{mode:08x}{uid:08x}{gid:08x}{nlink:08x}{mtime:08x}{filesize:08x}{devmajor:08x}{devminor:08x}{rdevmajor:08x}{rdevminor:08x}{namesize:08x}{check:08x}",
        ino = 0,
        mode = 0,
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
    let mut entry = header.into_bytes();
    entry.extend_from_slice(name_bytes);
    entry.push(0);
    pad4(&mut entry);
    entry
}

fn pad4(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
}

fn decision(archive: &[u8]) -> LinuxControllerEvidence {
    let (ide, lsi) = cpio_driver_names(archive).expect("valid cpio");
    LinuxControllerEvidence {
        ide,
        lsi,
        candidates: 1,
        decoded: 1,
        unreadable: 0,
    }
}

fn cpio_driver_names(archive: &[u8]) -> Option<(bool, bool)> {
    parse_cpio_archive(archive).map(|(drivers, _)| drivers)
}

#[test]
fn selects_ide_for_ata_piix() {
    assert_eq!(
        decision(&cpio_archive(&[
            "lib/modules/3.10/kernel/drivers/ata/ata_piix.ko.xz",
        ]))
        .decision()
        .adapter,
        VmdkAdapter::Ide
    );
}

#[test]
fn selects_lsi_when_mptspi_is_present() {
    let evidence = decision(&cpio_archive(&[
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
    let evidence = decision(&cpio_archive(&[
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
fn ide_wins_when_initramfs_contains_both_controller_families() {
    let evidence = decision(&cpio_archive(&[
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
    assert!(
        cpio_driver_names(&decode_initramfs(&gzip).unwrap())
            .unwrap()
            .1
    );

    let mut xz = xz2::write::XzEncoder::new(Vec::new(), 6);
    xz.write_all(&archive).unwrap();
    let xz = xz.finish().unwrap();
    assert!(
        cpio_driver_names(&decode_initramfs(&xz).unwrap())
            .unwrap()
            .1
    );

    let zstd = zstd::stream::encode_all(std::io::Cursor::new(&archive), 1).unwrap();
    assert!(
        cpio_driver_names(&decode_initramfs(&zstd).unwrap())
            .unwrap()
            .1
    );
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
    assert!(decode_initramfs(b"not-an-initramfs").is_none());
    let decision = LinuxControllerEvidence {
        candidates: 1,
        unreadable: 1,
        ..LinuxControllerEvidence::default()
    }
    .decision();
    assert_eq!(decision.adapter, VmdkAdapter::Ide);
    assert!(decision.reason.contains("could not be decoded"));
}

#[test]
fn rejects_truncated_cpio_without_guessing_a_driver() {
    let mut archive = cpio_archive(&["kernel/drivers/ata/ata_piix.ko"]);
    archive.truncate(archive.len() - cpio_archive_entry("TRAILER!!!").len());
    assert!(cpio_driver_names(&archive).is_none());
}
