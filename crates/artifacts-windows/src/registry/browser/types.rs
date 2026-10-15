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
    pub(super) subkeys: u32,
    pub(super) values: u32,
    pub(super) parent: u32,
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
    pub(super) inline: bool,
    pub(super) data: u32,
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
    pub(super) bytes: Arc<Vec<u8>>,
    pub(super) cells: Vec<u64>,
    pub(super) root: u32,
    pub(super) end: usize,
    pub(super) minor: u32,
    pub(super) partial: bool,
    pub(super) dirty: bool,
    pub(super) provenance: Vec<ProvenanceSpan>,
}

#[derive(Debug, Clone, Copy)]
pub struct ProvenanceSpan {
    pub start: usize,
    pub end: usize,
}
