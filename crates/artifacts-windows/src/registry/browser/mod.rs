//! Bounded, read-only base-hive browsing. No transaction replay or recovery.
mod cells;
mod data;
mod decode;
mod keys;
mod search;
mod types;
mod values;

pub use types::{
    BrowserError, BrowserKey, BrowserPage, BrowserSearchHit, BrowserSearchPage, BrowserValue,
    HiveSnapshot, ValueRange,
};
