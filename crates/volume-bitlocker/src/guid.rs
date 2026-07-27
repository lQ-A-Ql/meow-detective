//! Microsoft mixed-endian GUID formatting.
//!
//! Derived from `bitlocker-core`'s `guid` module (see `../NOTICE`).
//!
//! A Windows GUID stores its first three fields little-endian and its final
//! eight bytes big-endian. This renders the canonical lowercase `8-4-4-4-12`
//! form, matching what `libbde` and `pybde` print, so a volume GUID in a report
//! can be compared against those tools by eye.

use std::fmt::Write;

use crate::bytes::{le_u16, le_u32};

/// Renders a 16-byte Microsoft GUID in canonical lowercase `8-4-4-4-12` form.
#[must_use]
pub fn format_guid(raw: &[u8; 16]) -> String {
    let first = le_u32(raw, 0);
    let second = le_u16(raw, 4);
    let third = le_u16(raw, 6);
    let mut tail = String::with_capacity(20);
    for (index, byte) in raw[8..16].iter().enumerate() {
        if index == 2 {
            tail.push('-');
        }
        // Writing to a String cannot fail.
        let _ = write!(tail, "{byte:02x}");
    }
    format!("{first:08x}-{second:04x}-{third:04x}-{tail}")
}

#[cfg(test)]
#[path = "../tests/unit/guid.rs"]
mod tests;
