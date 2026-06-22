//! Typed errors for the bounded EVTX boot/shutdown parser.

use thiserror::Error;

/// Errors that can occur while extracting boot/shutdown candidates from a
/// System.evtx file.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EvtxBootError {
    /// The source path is not the bounded System.evtx surface this parser supports.
    #[error("{path} is outside bounded System.evtx boot/shutdown parser scope")]
    UnsupportedPath { path: String },

    /// The input exceeds the bounded parser size limit.
    #[error("{path} exceeds bounded EVTX parser limit of {max} bytes (got {size})")]
    InputTooLarge {
        path: String,
        size: usize,
        max: usize,
    },

    /// The underlying `evtx` parser failed to initialize.
    #[error("{path} EVTX parser initialization failed: {detail}")]
    ParserInit { path: String, detail: String },

    /// A chunk-level parse failure was reported while streaming records.
    #[error(
        "{path} EVTX chunk parse warning: chunk={chunk_id} offset=0x{offset:08X} reason={detail}"
    )]
    ChunkParse {
        path: String,
        chunk_id: u64,
        offset: u64,
        detail: String,
    },

    /// A record-level parse failure was reported while streaming records.
    #[error("{path} EVTX record parse warning: record={record_id:?} reason={detail}")]
    RecordParse {
        path: String,
        record_id: Option<u64>,
        detail: String,
    },

    /// No supported boot/shutdown candidate events were found.
    #[error("{path} contains no supported boot/shutdown candidate events")]
    NoSupportedEvents { path: String },
}
