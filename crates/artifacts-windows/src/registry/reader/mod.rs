//! Immutable, bounded registry hive reading shared by queries and browsing.
pub(crate) mod cells;
mod class_name;
mod data;
pub(crate) mod decode;
mod keys;
mod provenance;
mod types;
mod values;

pub use types::{
    BrowserError, BrowserKey, BrowserOverlaySource, BrowserPage, BrowserSearchHit,
    BrowserSearchPage, BrowserValue, HiveSnapshot, ProvenanceSpan, ValueRange,
};

pub type HiveView = HiveSnapshot;
