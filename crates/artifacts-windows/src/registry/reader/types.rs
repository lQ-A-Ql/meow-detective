use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BrowserError {
    #[error("registry search cancelled")]
    Cancelled,
    #[error("registry hive format is unsupported")]
    Unsupported,
    #[error("registry cell is corrupt")]
    Corrupt,
    #[error("registry item was not found")]
    NotFound,
    #[error("registry request exceeds the supported bounds")]
    Limit,
    #[error("registry path is invalid")]
    InvalidPath,
}

#[derive(Debug, Clone)]
pub struct BrowserKey {
    pub offset: u32,
    pub name: String,
    pub path: String,
    pub filetime: Option<u64>,
    pub subkey_count: u32,
    pub value_count: u32,
    pub(crate) subkeys: u32,
    pub(crate) values: u32,
    pub(crate) parent: u32,
    pub(crate) class_data: u32,
    pub(crate) class_length: u16,
    pub overlay_source: BrowserOverlaySource,
}

#[derive(Debug, Clone)]
pub struct BrowserValue {
    pub offset: u32,
    pub name: String,
    pub type_code: u32,
    pub byte_length: u32,
    pub preview: String,
    pub data_error: Option<BrowserError>,
    pub overlay_source: BrowserOverlaySource,
    pub data_overlay_source: BrowserOverlaySource,
    pub(crate) inline: bool,
    pub(crate) data: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserOverlaySource {
    Base,
    Recovered,
    Merged,
}

#[derive(Debug)]
pub struct BrowserPage<T> {
    pub declared: u32,
    pub skipped: u32,
    pub items: Vec<(u32, Result<T, BrowserError>)>,
    pub next: Option<usize>,
    pub diagnostic: Option<BrowserError>,
}

#[derive(Debug)]
pub struct ValueRange {
    pub bytes: Vec<u8>,
    pub total_length: u32,
    pub decoded: Option<String>,
    pub partial: bool,
    pub overlay_source: BrowserOverlaySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSearchHit {
    pub kind: &'static str,
    pub path: String,
    pub name: String,
    pub cell_offset: u32,
    pub matched_field: &'static str,
    pub preview: String,
    pub key_offset: u32,
    pub value_index: Option<u32>,
    pub data_offset: Option<u64>,
    pub overlay_source: BrowserOverlaySource,
    pub data_overlay_source: BrowserOverlaySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSearchPage {
    pub hits: Vec<BrowserSearchHit>,
    pub scanned_keys: u32,
    pub scanned_values: u32,
    pub skipped: u32,
    pub truncated: bool,
    pub total_matches: u64,
    pub complete: bool,
}

/// Immutable source bytes plus a compact allocation-boundary bitmap. A cell
/// reference must identify an allocated cell start, never bytes within another.
#[derive(Debug, Clone)]
pub struct HiveSnapshot {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) cells: Arc<Vec<u64>>,
    pub(crate) root: u32,
    pub(crate) end: usize,
    pub(crate) minor: u32,
    pub(crate) partial: bool,
    pub(crate) dirty: bool,
    pub(crate) provenance: Vec<ProvenanceSpan>,
}

#[derive(Debug, Clone, Copy)]
pub struct ProvenanceSpan {
    pub start: usize,
    pub end: usize,
}
