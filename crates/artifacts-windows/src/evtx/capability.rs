//! Structured EVTX event coverage. The EVTX parser validates file contents.

pub const EVTX_PARSER_ID: &str = "evtx.boot_shutdown";
pub const SUPPORTED_EVENT_IDS: &[u32] = &[
    12, 13, 6005, 6006, 6008, 1074, 4104, 1, 21, 1116, 4624, 4625, 4648, 4688, 4698, 4702, 4720,
    4732, 1000, 1001, 1002, 1033, 11707, 11708,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvtxCapability {
    pub parser_id: &'static str,
    pub supported_event_ids: &'static [u32],
    pub max_input_bytes: usize,
}

pub fn evtx_capability() -> EvtxCapability {
    EvtxCapability {
        parser_id: EVTX_PARSER_ID,
        supported_event_ids: SUPPORTED_EVENT_IDS,
        max_input_bytes: super::parser::MAX_EVTX_ANALYSIS_BYTES,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evtx_capability.rs"]
mod tests;
