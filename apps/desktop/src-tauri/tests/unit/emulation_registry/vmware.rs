use std::path::Path;

use super::{
    discover, vmrun_lists_vmx, vmware_compatible_path, vmx_log_has_exited_since,
    vmx_log_has_tools_started_since, VmwareError,
};

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

#[test]
fn control_timeout_identifies_the_operation_and_window() {
    let error = VmwareError::ControlTimeout {
        operation: "start".to_string(),
        timeout_secs: 120,
    };

    assert_eq!(
        error.to_string(),
        "VMware start command timed out after 120s"
    );
}

#[test]
fn vmware_log_exit_marker_requires_a_complete_vmx_exit_line() {
    assert!(!vmx_log_has_exited_since("VMX exit", Some(0)));
    assert!(!vmx_log_has_exited_since(
        "VMX exited unexpectedly",
        Some(0)
    ));
    assert!(vmx_log_has_exited_since(
        "2026-08-27T10:26:02 VMX exit (0)",
        Some(0)
    ));
    assert!(vmx_log_has_exited_since("  vmx exit (1)", Some(0)));
}

#[test]
fn vmware_log_exit_marker_must_be_after_the_launch_baseline() {
    let stale = "old VMX exit (0)\n";
    let baseline = u64::try_from(stale.len()).unwrap();
    assert!(!vmx_log_has_exited_since(stale, Some(baseline)));
    let current = format!("{stale}current VMX exit (0)\n");
    assert!(vmx_log_has_exited_since(&current, Some(baseline)));
}

#[test]
fn vmware_tools_marker_must_be_from_the_current_session() {
    let stale = "Tools: Running status rpc handler: 0 => 1.\n";
    let baseline = u64::try_from(stale.len()).unwrap();
    assert!(!vmx_log_has_tools_started_since(stale, Some(baseline)));
    let current = format!("{stale}Tools: Running status rpc handler: 0 => 1.\n");
    assert!(vmx_log_has_tools_started_since(&current, Some(baseline)));

    let stale_crlf = "Tools: Running status rpc handler: 0 => 1.\r\n";
    let baseline = u64::try_from(stale_crlf.len()).unwrap();
    let current_crlf = format!("{stale_crlf}Tools: Running status rpc handler: 0 => 1.\r\n");
    assert!(vmx_log_has_tools_started_since(
        &current_crlf,
        Some(baseline)
    ));
}

#[test]
fn vmrun_listing_matches_only_the_requested_vmx_path() {
    let output =
        b"Total running VMs: 2\r\nC:\\Case\\one\\machine.vmx\r\nC:\\Case\\two\\machine.vmx\r\n";
    assert!(vmrun_lists_vmx(output, "c:\\case\\one\\machine.vmx"));
    assert!(!vmrun_lists_vmx(output, "C:\\Case\\other\\machine.vmx"));
}

#[test]
fn vmx_exit_timeout_is_safe_to_show_without_a_host_path() {
    let error = VmwareError::VmxExitTimeout { timeout_secs: 30 };
    assert_eq!(
        error.to_string(),
        "VMware VMX exit could not be confirmed within 30s"
    );
}
