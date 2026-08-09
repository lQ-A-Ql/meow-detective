use super::*;

use std::io::{Read, Seek, SeekFrom};

use evidence_core::ReaderInfo;

struct MemoryReader {
    bytes: Vec<u8>,
    position: u64,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(bytes: Vec<u8>) -> Self {
        let info = ReaderInfo {
            path: std::path::PathBuf::from("<memory>"),
            size: bytes.len() as u64,
            kind: "memory".to_string(),
        };
        Self {
            bytes,
            position: 0,
            info,
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let available = self.bytes.len() as u64 - self.position.min(self.bytes.len() as u64);
        let wanted = available.min(buffer.len() as u64) as usize;
        let start = self.position as usize;
        buffer[..wanted].copy_from_slice(&self.bytes[start..start + wanted]);
        self.position += wanted as u64;
        Ok(wanted)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(delta) => self.position as i128 + delta as i128,
            SeekFrom::End(delta) => self.bytes.len() as i128 + delta as i128,
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn open_fs(image: Vec<u8>) -> fs_ext4::Ext4Reader {
    let reader: Box<dyn EvidenceReader> = Box::new(MemoryReader::new(image));
    fs_ext4::Ext4Reader::open(reader, 0).expect("synthetic ext4 opens")
}

#[test]
fn parses_os_release_fields() {
    let content =
        b"NAME=\"CentOS Linux\"\nID=\"centos\"\nPRETTY_NAME=\"CentOS Linux 7 (Core)\"\n# comment\n";
    assert_eq!(
        parse_os_release_pretty_name(content).as_deref(),
        Some("CentOS Linux 7 (Core)")
    );
    assert_eq!(parse_os_release_id(content).as_deref(), Some("centos"));
    assert_eq!(parse_os_release_pretty_name(b"ID=ubuntu\n"), None);
    assert_eq!(
        parse_os_release_id(b"ID=debian\n").as_deref(),
        Some("debian")
    );
}

#[test]
fn maps_distro_ids_to_vmware_guestids() {
    assert_eq!(linux_guest_os_id(Some("ubuntu")), "ubuntu-64");
    assert_eq!(linux_guest_os_id(Some("debian")), "debian12-64");
    assert_eq!(linux_guest_os_id(Some("rhel")), "rhel8-64");
    assert_eq!(linux_guest_os_id(Some("centos")), "centos-64");
    assert_eq!(linux_guest_os_id(Some("ol")), "oraclelinux-64");
    assert_eq!(linux_guest_os_id(Some("arch")), "other5xlinux-64");
    assert_eq!(linux_guest_os_id(None), "other5xlinux-64");
}

#[test]
fn probes_a_synthetic_linux_root() {
    let fs = open_fs(testing::builders::ext4::linux_root_ext4_image());
    let probe = probe_linux_fs_root(&fs, "Ext4").expect("probe succeeds");
    assert_eq!(probe.pretty_name.as_deref(), Some("CentOS Linux 7 (Core)"));
    assert!(probe.kernel_present);
    assert!(probe.fstab_present);
    assert!(probe.risk_notes.is_empty());
}

#[test]
fn flags_btrfs_roots_and_missing_boot_assets() {
    let fs = open_fs(testing::builders::ext4::linux_root_ext4_image());
    let probe = probe_linux_fs_root(&fs, "Btrfs").expect("probe succeeds");
    assert!(probe.risk_notes.iter().any(|note| note == "btrfs-root"));

    let fs = open_fs(testing::builders::ext4::minimal_ext4_image());
    let probe = probe_linux_fs_root(&fs, "Ext4");
    assert!(probe.is_none(), "a root without /etc is not a system root");
}

#[test]
fn flags_missing_kernel_and_fstab() {
    let fs = open_fs(testing::builders::ext4::linux_root_ext4_image_without_boot_assets());
    let probe = probe_linux_fs_root(&fs, "Ext4").expect("probe succeeds");
    assert!(!probe.kernel_present);
    assert!(!probe.fstab_present);
    assert!(probe.risk_notes.iter().any(|note| note == "no-kernel"));
    assert!(probe.risk_notes.iter().any(|note| note == "no-fstab"));
    assert!(!probe.risk_notes.iter().any(|note| note == "no-init"));
}

#[test]
fn reads_os_release_from_the_usr_lib_fallback() {
    let fs = open_fs(testing::builders::ext4::linux_root_ext4_image_usr_lib_os_release());
    let probe = probe_linux_fs_root(&fs, "Ext4").expect("probe succeeds");
    assert_eq!(probe.pretty_name.as_deref(), Some("CentOS Linux 7 (Core)"));
    assert!(probe.kernel_present);
    assert!(!probe.fstab_present);
    assert!(probe.risk_notes.iter().any(|note| note == "no-fstab"));
}
