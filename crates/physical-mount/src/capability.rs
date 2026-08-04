#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalMountCapability {
    pub available: bool,
    pub backend: &'static str,
    pub reason: Option<String>,
}

#[cfg(windows)]
pub fn physical_mount_capability() -> PhysicalMountCapability {
    use windows_sys::Win32::Storage::IscsiDisc::{GetIScsiVersionInformation, ISCSI_VERSION_INFO};

    let mut version = ISCSI_VERSION_INFO::default();
    // SAFETY: `version` points to initialized writable storage for the duration
    // of the synchronous Windows API call.
    let result = unsafe { GetIScsiVersionInformation(&mut version) };
    if result == 0 {
        PhysicalMountCapability {
            available: true,
            backend: "windowsIscsi",
            reason: None,
        }
    } else {
        PhysicalMountCapability {
            available: false,
            backend: "windowsIscsi",
            reason: Some(format!(
                "Microsoft iSCSI Initiator API is unavailable (code {result})"
            )),
        }
    }
}

#[cfg(not(windows))]
pub fn physical_mount_capability() -> PhysicalMountCapability {
    PhysicalMountCapability {
        available: false,
        backend: "unsupported",
        reason: Some("physical-disk mounting requires Windows".to_string()),
    }
}
