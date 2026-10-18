//! Bounded read-only carving. This module owns no database, host paths or UI.
mod png;
mod png_stream;
mod scan;
mod types;

pub use scan::scan_png;
pub use types::{
    CarveCandidate, CarveError, CarveLimits, CarveScope, CarveStatus, ScanReport, ScanRequest,
};
pub const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
pub const PNG_VALIDATOR_VERSION: &str = "png-crc-zlib-8bit-v1";

#[cfg(test)]
#[path = "../../tests/unit/carving.rs"]
mod tests;
