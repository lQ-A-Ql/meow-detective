use std::path::Path;

use super::{discover, vmware_compatible_path};

#[test]
fn discovery_never_falls_back_to_an_unrelated_virtualization_backend() {
    match discover() {
        Ok((workstation, vmrun)) => {
            assert_eq!(workstation.file_name().unwrap(), "vmware.exe");
            assert_eq!(vmrun.file_name().unwrap(), "vmrun.exe");
        }
        Err(error) => assert!(error.to_string().contains("VMware Workstation")),
    }
}

#[test]
#[cfg(windows)]
fn vmware_launch_path_removes_verbatim_disk_prefix() {
    let path = Path::new(r"\\?\C:\Cases\取证案件\machine.vmx");

    assert_eq!(
        vmware_compatible_path(path),
        Path::new(r"C:\Cases\取证案件\machine.vmx")
    );
}

#[test]
#[cfg(windows)]
fn vmware_launch_path_removes_verbatim_unc_prefix() {
    let path = Path::new(r"\\?\UNC\server\cases\machine.vmx");

    assert_eq!(
        vmware_compatible_path(path),
        Path::new(r"\\server\cases\machine.vmx")
    );
}

#[test]
fn vmware_launch_path_preserves_ordinary_paths() {
    let path = Path::new(r"C:\Cases\machine.vmx");

    assert_eq!(vmware_compatible_path(path), path);
}
