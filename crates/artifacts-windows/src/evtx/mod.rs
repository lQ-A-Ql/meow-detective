//! Targeted Windows EVTX extraction for analysis views.

pub mod capability;
pub mod error;
pub mod parser;

pub use capability::{
    evtx_capability, EvtxCapability, EVTX_PARSER_ID, SUPPORTED_EVENT_IDS,
    SUPPORTED_SOURCE_PATH_SUFFIX,
};
pub use error::EvtxBootError;
pub use parser::{
    extract_boot_shutdown_events, extract_boot_shutdown_events_from_json_records,
    extract_structured_events, visit_structured_events_from_read_seek, EvtxApplicationEvent,
    EvtxApplicationEventKind, EvtxBootEvent, EvtxBootEventKind, EvtxBootExtraction,
    EvtxSecurityEvent, EvtxSecurityEventKind, EvtxStructuredEvent, EvtxStructuredExtraction,
    EvtxVisitError, EvtxVisitSummary, MAX_EVTX_ANALYSIS_BYTES,
};
