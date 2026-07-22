//! Guard describing the intentionally narrow EVTX capability surface.
//!
//! The workspace ships a bounded adapter over the local patched `evtx` fork.
//! This module does not provide a generic EVTX extraction service; it documents
//! the active capability so callers can fail closed instead of assuming every
//! `.evtx` file or event id is supported.

pub const EVTX_PARSER_ID: &str = "evtx.boot_shutdown";
pub const SUPPORTED_EVENT_IDS: &[u32] = &[
    12, 13, 6005, 6006, 6008, 1074, // boot/shutdown
    4104, // PowerShell script block logging
    1,    // Sysmon process creation
    21,   // RDP session connect
    1116, // Windows Defender malware detection
    // Security audit events
    4624, 4625, 4648, 4688, 4698, 4702, 4720, 4732, // Application events
    1000, 1001, 1002, 1033, 11707, 11708,
];
pub const SUPPORTED_SOURCE_PATH_SUFFIX: &str = "windows/system32/winevt/logs/system.evtx";

/// Extended set of EVTX log paths supported by the parser.
/// Includes the original System.evtx, Security.evtx, Application.evtx plus
/// forensically-critical operational channels.  In addition, any `.evtx`
/// file located under `Windows/System32/winevt/Logs` is accepted so that
/// adjacent channels (Biometrics, Winlogon, etc.) do not generate noisy
/// "outside bounded parser scope" warnings; events from those channels are
/// simply ignored unless they match a supported ID/channel rule.
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
    if !normalized.ends_with(".evtx") {
        return false;
    }
    // Accept bare filenames used by enumerated operational channels.
    let bare_filename_match = SUPPORTED_SOURCE_PATH_SUFFIXES.iter().any(|suffix| {
        let sfx = suffix.replace('\\', "/");
        normalized == sfx
    });
    if bare_filename_match {
        return true;
    }
    // Only accept files that live in the standard Windows event-log directory.
    // The directory check includes a leading slash so arbitrary parent prefixes
    // such as `c:/notwindows/system32/winevt/logs` are not accepted.
    if let Some((dir, _file)) = normalized.rsplit_once('/') {
        return dir == "windows/system32/winevt/logs"
            || dir.ends_with("/windows/system32/winevt/logs");
    }
    false
}

#[cfg(test)]
#[path = "../../tests/unit/evtx_capability.rs"]
mod tests;
