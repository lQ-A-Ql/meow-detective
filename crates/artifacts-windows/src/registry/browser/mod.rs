//! Bounded, read-only base-hive browsing. No transaction replay or recovery.
mod cells;
mod data;
mod decode;
mod hardware;
mod keys;
mod search;
mod search_walk;
mod types;
mod values;

pub use types::{
    BrowserError, BrowserKey, BrowserOverlaySource, BrowserPage, BrowserSearchHit,
    BrowserSearchPage, BrowserValue, HiveSnapshot, ProvenanceSpan, ValueRange,
};
