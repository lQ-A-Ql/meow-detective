#![cfg(windows)]

use std::io::{Read, Seek, SeekFrom, Write};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use physical_mount::{PhysicalImageKind, PhysicalMount};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW, QUERY_SERVICE_CONFIGW,
    SC_HANDLE, SC_MANAGER_CONNECT, SERVICE_QUERY_CONFIG,
};

#[test]
#[ignore = "requires an elevated Windows session and the Microsoft iSCSI Initiator service"]
fn loopback_iscsi_exposes_a_read_only_physical_disk() {
    let original_service_start_type = iscsi_service_start_type();
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
    assert_eq!(iscsi_service_start_type(), original_service_start_type);
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

fn iscsi_service_start_type() -> u32 {
    let service_name = "MSiSCSI"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: null machine/database pointers select the local default SCM.
    let manager = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
    assert!(!manager.is_null(), "OpenSCManagerW failed");
    let manager = ServiceHandle(manager);
    // SAFETY: manager and the null-terminated service-name buffer are valid.
    let service = unsafe { OpenServiceW(manager.0, service_name.as_ptr(), SERVICE_QUERY_CONFIG) };
    assert!(!service.is_null(), "OpenServiceW failed");
    let service = ServiceHandle(service);
    let mut required = 0u32;
    // SAFETY: a null buffer with size zero is the documented size probe.
    let result = unsafe { QueryServiceConfigW(service.0, null_mut(), 0, &mut required) };
    // SAFETY: GetLastError immediately follows the failed size probe.
    let code = unsafe { GetLastError() };
    assert_eq!(result, 0);
    assert_eq!(code, ERROR_INSUFFICIENT_BUFFER);
    let words = (required as usize).div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0usize; words];
    let config = buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    // SAFETY: the aligned buffer contains at least `required` writable bytes.
    let result = unsafe { QueryServiceConfigW(service.0, config, required, &mut required) };
    assert_ne!(result, 0, "QueryServiceConfigW failed");
    // SAFETY: QueryServiceConfigW initialized the fixed structure prefix.
    unsafe { (*config).dwStartType }
}

struct ServiceHandle(SC_HANDLE);

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: this guard owns the service handle.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
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
