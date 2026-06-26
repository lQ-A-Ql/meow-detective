//! Guard describing the intentionally narrow EVTX capability surface.
//!
//! The workspace ships a bounded adapter over the local patched `evtx` fork.
//! This module does not provide a generic EVTX extraction service; it documents
//! the active capability so callers can fail closed instead of assuming every
//! `.evtx` file or event id is supported.

pub const EVTX_PARSER_ID: &str = "evtx.boot_shutdown";
pub const SUPPORTED_EVENT_IDS: &[u32] = &[
    6005, 6006, 6008, 1074, // boot/shutdown
    4104, // PowerShell script block logging
    1,    // Sysmon process creation
    21,   // RDP session connect
    1116, // Windows Defender malware detection
];
pub const SUPPORTED_SOURCE_PATH_SUFFIX: &str = "windows/system32/winevt/logs/system.evtx";

/// Extended set of EVTX log paths supported by the parser.
/// Includes the original System.evtx plus forensically-critical channels.
pub const SUPPORTED_SOURCE_PATH_SUFFIXES: &[&str] = &[
    "windows/system32/winevt/logs/system.evtx",
    "windows/system32/winevt/logs/security.evtx",
    "windows/system32/winevt/logs/application.evtx",
    "microsoft-windows-powershell%4operational.evtx",
    "microsoft-windows-sysmon%4operational.evtx",
    "microsoft-windows-terminalservices-localsessionmanager%4operational.evtx",
    "microsoft-windows-windows defender%4operational.evtx",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvtxCapability {
    pub parser_id: &'static str,
    pub supported_event_ids: &'static [u32],
    pub source_path_suffix: &'static str,
    pub source_path_suffixes: &'static [&'static str],
    pub max_input_bytes: usize,
}

pub fn evtx_capability() -> EvtxCapability {
    EvtxCapability {
        parser_id: EVTX_PARSER_ID,
        supported_event_ids: SUPPORTED_EVENT_IDS,
        source_path_suffix: SUPPORTED_SOURCE_PATH_SUFFIX,
        source_path_suffixes: SUPPORTED_SOURCE_PATH_SUFFIXES,
        max_input_bytes: super::parser::MAX_EVTX_ANALYSIS_BYTES,
    }
}

pub fn supports_evtx_boot_shutdown_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    SUPPORTED_SOURCE_PATH_SUFFIXES.iter().any(|suffix| {
        let sfx = suffix.replace('\\', "/");
        normalized == sfx || normalized.ends_with(&format!("/{sfx}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_documents_bounded_evtx_surface() {
        let capability = evtx_capability();
        assert_eq!(capability.parser_id, "evtx.boot_shutdown");
        assert_eq!(
            capability.supported_event_ids,
            &[6005, 6006, 6008, 1074, 4104, 1, 21, 1116]
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
    fn path_guard_rejects_unsupported_evtx() {
        assert!(!supports_evtx_boot_shutdown_path(
            "C:/notWindows/System32/winevt/Logs/System.evtx"
        ));
        assert!(!supports_evtx_boot_shutdown_path(
            "C:/Windows/System32/winevt/Logs/UnknownChannel.evtx"
        ));
    }
}
