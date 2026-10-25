//! Bounded, read-only base-hive browsing. No transaction replay or recovery.
mod hardware;
mod search;
mod search_walk;

use super::reader::decode;
pub use super::reader::{
    BrowserError, BrowserKey, BrowserOverlaySource, BrowserPage, BrowserSearchHit,
    BrowserSearchPage, BrowserValue, HiveSnapshot, ProvenanceSpan, ValueRange,
};
