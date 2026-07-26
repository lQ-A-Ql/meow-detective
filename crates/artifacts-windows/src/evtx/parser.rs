//! Bounded EVTX candidate extraction.
//!
//! The parser is split by stable capability: public data contracts, bounded
//! binary reading, and JSON record projection.

mod reader;
mod records;
mod types;

pub use reader::{
    extract_boot_shutdown_events, extract_boot_shutdown_events_from_json_records,
    extract_structured_events, extract_structured_events_from_json_records,
    extract_structured_events_from_read_seek, probe_newest_records,
    visit_structured_events_from_read_seek, EvtxVisitError, EvtxVisitSummary,
    MAX_EVTX_ANALYSIS_BYTES,
};
pub use types::{
    EvtxApplicationEvent, EvtxApplicationEventKind, EvtxBootEvent, EvtxBootEventKind,
    EvtxBootExtraction, EvtxEventCategory, EvtxSecurityEvent, EvtxSecurityEventKind,
    EvtxStructuredEvent, EvtxStructuredExtraction,
};

#[cfg(test)]
#[path = "../../tests/unit/evtx_parser.rs"]
mod tests;
