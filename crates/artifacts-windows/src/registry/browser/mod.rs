//! Bounded, read-only base-hive browsing. No transaction replay or recovery.
mod cells;
mod data;
mod decode;
mod keys;
mod types;
mod values;

pub use types::{BrowserError, BrowserKey, BrowserPage, BrowserValue, HiveSnapshot, ValueRange};
