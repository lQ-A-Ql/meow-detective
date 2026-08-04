#![cfg(windows)]

use std::io::{Read, Seek, SeekFrom, Write};
use std::time::{Duration, Instant};

use physical_mount::{PhysicalImageKind, PhysicalMount};

#[test]
#[ignore = "requires an elevated Windows session and the Microsoft iSCSI Initiator service"]
fn loopback_iscsi_exposes_a_read_only_physical_disk() {
    let mut fixture = tempfile::NamedTempFile::new().expect("raw fixture");
    let original = minimal_mbr_disk();
    fixture.write_all(&original).expect("write raw fixture");
    fixture.flush().expect("flush raw fixture");

    let mut mount = PhysicalMount::start(fixture.path(), PhysicalImageKind::Raw)
        .expect("loopback physical mount must start");
    let device_path = mount
        .physical_device_path()
        .expect("Windows physical device path")
        .to_string();
    let mut physical = std::fs::OpenOptions::new()
        .read(true)
        .open(&device_path)
        .expect("physical disk must open read-only");
    let mut sector = [0u8; 512];
    physical.read_exact(&mut sector).expect("read MBR sector");
    assert_eq!(&sector, &original[..512]);
    drop(physical);

    if let Ok(mut writable) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&device_path)
    {
        writable
            .seek(SeekFrom::Start(0))
            .expect("seek physical disk");
        assert!(writable.write_all(&[0xA5; 512]).is_err());
    }

    mount.stop().expect("physical mount must stop");
    let release_deadline = Instant::now() + Duration::from_secs(10);
    let mut released = false;
    while Instant::now() < release_deadline {
        if std::fs::OpenOptions::new()
            .read(true)
            .open(&device_path)
            .is_err()
        {
            released = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(released, "physical disk remained visible after logout");
    let after = std::fs::read(fixture.path()).expect("read source fixture");
    assert_eq!(after, original);
}

fn minimal_mbr_disk() -> Vec<u8> {
    const DISK_BYTES: usize = 4 * 1024 * 1024;
    let mut disk = vec![0u8; DISK_BYTES];
    let partition = &mut disk[446..462];
    partition[4] = 0x07;
    partition[8..12].copy_from_slice(&2048u32.to_le_bytes());
    partition[12..16].copy_from_slice(&4096u32.to_le_bytes());
    disk[510] = 0x55;
    disk[511] = 0xAA;
    disk
}
