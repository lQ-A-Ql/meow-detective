use super::*;

#[test]
fn capability_documents_bounded_evtx_surface() {
    let capability = evtx_capability();
    assert_eq!(capability.parser_id, "evtx.boot_shutdown");
    assert_eq!(
        capability.supported_event_ids,
        &[
            12, 13, 6005, 6006, 6008, 1074, 4104, 1, 21, 1116, 4624, 4625, 4648, 4688, 4698, 4702,
            4720, 4732, 1000, 1001, 1002, 1033, 11707, 11708
        ]
    );
    assert_eq!(
        capability.source_path_suffix,
        "windows/system32/winevt/logs/system.evtx"
    );
    assert!(capability.source_path_suffixes.len() >= 7);
    assert!(capability.max_input_bytes > 0);
}

#[test]
fn path_guard_accepts_system_evtx_suffix() {
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/System.evtx"
    ));
    assert!(supports_evtx_boot_shutdown_path(
        "Windows\\System32\\winevt\\Logs\\System.evtx"
    ));
}

#[test]
fn path_guard_accepts_security_and_application() {
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Security.evtx"
    ));
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Application.evtx"
    ));
}

#[test]
fn path_guard_accepts_powershell_operational() {
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Microsoft-Windows-PowerShell%4Operational.evtx"
    ));
}

#[test]
fn path_guard_accepts_sysmon_operational() {
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Microsoft-Windows-Sysmon%4Operational.evtx"
    ));
}

#[test]
fn path_guard_accepts_terminalservices_operational() {
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Microsoft-Windows-TerminalServices-LocalSessionManager%4Operational.evtx"
    ));
}

#[test]
fn path_guard_accepts_defender_operational() {
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Microsoft-Windows-Windows Defender%4Operational.evtx"
    ));
}

#[test]
fn path_guard_accepts_any_evtx_under_winevt_logs() {
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Microsoft-Windows-Biometrics%4Operational.evtx"
    ));
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Microsoft-Windows-Winlogon%4Operational.evtx"
    ));
    assert!(supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/UnknownChannel.evtx"
    ));
}

#[test]
fn path_guard_rejects_evtx_outside_winevt_logs() {
    assert!(!supports_evtx_boot_shutdown_path(
        "C:/notWindows/System32/winevt/Logs/System.evtx"
    ));
    assert!(!supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/config/SYSTEM"
    ));
    assert!(!supports_evtx_boot_shutdown_path(
        "C:/Windows/System32/winevt/Logs/Setup.etl"
    ));
}
