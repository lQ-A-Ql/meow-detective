//! Targeted Windows EVTX extraction for analysis views.

pub mod parser;

pub use parser::{
    extract_boot_shutdown_events, extract_boot_shutdown_events_from_json_records, EvtxBootEvent,
    EvtxBootEventKind, EvtxBootExtraction, MAX_EVTX_ANALYSIS_BYTES,
};
