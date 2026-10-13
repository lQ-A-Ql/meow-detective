use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BrowserError {
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
}

#[derive(Debug, Clone)]
pub struct BrowserValue {
    pub offset: u32,
    pub name: String,
    pub type_code: u32,
    pub byte_length: u32,
    pub preview: String,
    pub data_error: Option<BrowserError>,
    pub(super) inline: bool,
    pub(super) data: u32,
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
}

/// Immutable source bytes plus a compact allocation-boundary bitmap. A cell
/// reference must identify an allocated cell start, never bytes within another.
#[derive(Debug)]
pub struct HiveSnapshot {
    pub(super) bytes: Arc<Vec<u8>>,
    pub(super) cells: Vec<u64>,
    pub(super) root: u32,
    pub(super) end: usize,
    pub(super) minor: u32,
    pub(super) partial: bool,
    pub(super) dirty: bool,
}
