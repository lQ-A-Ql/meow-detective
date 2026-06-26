//! Targeted Windows EVTX extraction for analysis views.

pub mod capability;
pub mod error;
pub mod parser;

pub use capability::{
    evtx_capability, supports_evtx_boot_shutdown_path, EvtxCapability, EVTX_PARSER_ID,
    SUPPORTED_EVENT_IDS, SUPPORTED_SOURCE_PATH_SUFFIX, SUPPORTED_SOURCE_PATH_SUFFIXES,
};
pub use error::EvtxBootError;
pub use parser::{
    extract_boot_shutdown_events, extract_boot_shutdown_events_from_json_records, EvtxBootEvent,
    EvtxBootEventKind, EvtxBootExtraction, MAX_EVTX_ANALYSIS_BYTES,
};
