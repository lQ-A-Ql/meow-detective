//! Guard describing the intentionally narrow EVTX capability surface.
//!
//! The workspace ships a bounded boot/shutdown adapter over the local patched
//! `evtx` fork. This module does not provide a generic EVTX extraction service;
//! it documents the active capability so callers can fail closed instead of
//! assuming every `.evtx` file or event id is supported.

pub const EVTX_PARSER_ID: &str = "evtx.boot_shutdown";
pub const SUPPORTED_EVENT_IDS: &[u32] = &[6005, 6006, 6008, 1074];
pub const SUPPORTED_SOURCE_PATH_SUFFIX: &str = "windows/system32/winevt/logs/system.evtx";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvtxCapability {
    pub parser_id: &'static str,
    pub supported_event_ids: &'static [u32],
    pub source_path_suffix: &'static str,
    pub max_input_bytes: usize,
}

pub fn evtx_capability() -> EvtxCapability {
    EvtxCapability {
        parser_id: EVTX_PARSER_ID,
        supported_event_ids: SUPPORTED_EVENT_IDS,
        source_path_suffix: SUPPORTED_SOURCE_PATH_SUFFIX,
        max_input_bytes: super::parser::MAX_EVTX_ANALYSIS_BYTES,
    }
}

pub fn supports_evtx_boot_shutdown_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized == SUPPORTED_SOURCE_PATH_SUFFIX
        || normalized.ends_with(&format!("/{SUPPORTED_SOURCE_PATH_SUFFIX}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_documents_bounded_system_evtx_surface() {
        let capability = evtx_capability();
        assert_eq!(capability.parser_id, "evtx.boot_shutdown");
        assert_eq!(capability.supported_event_ids, &[6005, 6006, 6008, 1074]);
        assert_eq!(
            capability.source_path_suffix,
            "windows/system32/winevt/logs/system.evtx"
        );
        assert!(capability.max_input_bytes > 0);
    }

    #[test]
    fn path_guard_accepts_only_system_evtx_suffix() {
        assert!(supports_evtx_boot_shutdown_path(
            "C:/Windows/System32/winevt/Logs/System.evtx"
        ));
        assert!(supports_evtx_boot_shutdown_path(
            "Windows\\System32\\winevt\\Logs\\System.evtx"
        ));
        assert!(!supports_evtx_boot_shutdown_path(
            "C:/Windows/System32/winevt/Logs/Security.evtx"
        ));
        assert!(!supports_evtx_boot_shutdown_path(
            "C:/notWindows/System32/winevt/Logs/System.evtx"
        ));
    }
}
