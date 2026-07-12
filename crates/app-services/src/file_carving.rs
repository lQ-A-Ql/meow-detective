//! File carving from raw byte streams (unallocated space, disk images, etc.).
//!
//! Header/footer carving identifies files by their magic bytes and scans
//! for the corresponding footer terminator. This module supports:
//!
//! - **JPEG**: header `FF D8 FF`, footer `FF D9` (END-OF-IMAGE marker)
//! - **ZIP**: header `50 4B 03 04`, hunted to end of central directory
//! - **PDF**: header `%PDF` (`25 50 44 46`), footer `%%EOF`
//!
//! The public entry point is `carve_from_unallocated`, which takes any
//! `Read + Seek` source and returns a vector of carved files.

use std::io::{self, Read, Seek, SeekFrom};

// ── Public types ───────────────────────────────────────────────────────────

/// A single carved file recovered from a raw byte stream.
#[derive(Debug, Clone)]
pub struct CarvedFile {
    /// Detected file type (e.g. "jpeg", "zip", "pdf").
    pub file_type: String,
    /// Byte offset from the start of the stream where the file header was found.
    pub offset: u64,
    /// Total carved size in bytes (header through footer).
    pub size: u64,
    /// Raw bytes of the carved file.
    pub data: Vec<u8>,
    /// Confidence score in range 0.0–1.0.
    /// 1.0 = header and footer both present and well-formed.
    /// 0.5 = header found, footer heuristically placed (or > max size truncated).
    /// 0.3 = header found, no recognizable footer, truncated at max_extent.
    pub confidence: f64,
}

// ── Header/footer definitions ─────────────────────────────────────────────

/// Signature for a file format that can be carved.
struct CarveSignature {
    /// File type label.
    file_type: &'static str,
    /// Header magic bytes.
    header: &'static [u8],
    /// Footer magic bytes (searched for in forward direction from the header).
    footer: &'static [u8],
    /// Maximum reasonable file size in bytes. Carving stops at this extent
    /// if no footer is found.
    max_extent: u64,
}

/// Supported carve signatures.
const SIGNATURES: &[CarveSignature] = &[
    // JPEG: SOI marker FF D8 FF followed by any APP/JPG marker.
    // Footer: EOI marker FF D9.
    // Max extent: 100 MB (typical JPEGs are much smaller).
    CarveSignature {
        file_type: "jpeg",
        header: b"\xFF\xD8\xFF",
        footer: b"\xFF\xD9",
        max_extent: 100 * 1024 * 1024,
    },
    // ZIP (including OOXML, ODF, JAR, APK, etc.):
    // Header: local file header signature PK\003\004 (50 4B 03 04).
    // Footer: end of central directory marker PK\005\006 (50 4B 05 06),
    // hunted to find the end of the central directory.
    // Max extent: 500 MB.
    CarveSignature {
        file_type: "zip",
        header: b"\x50\x4B\x03\x04",
        footer: b"\x50\x4B\x05\x06",
        max_extent: 500 * 1024 * 1024,
    },
    // PDF: header "%PDF-x.y" where x,y are digits.
    // Footer: "%%EOF" (can be followed by optional whitespace/newline).
    // Max extent: 200 MB.
    CarveSignature {
        file_type: "pdf",
        header: b"%PDF",
        footer: b"%%EOF",
        max_extent: 200 * 1024 * 1024,
    },
    // GZIP: header 1F 8B 08, footer is the last 4 bytes (crc32 + ISIZE),
    // but for carving we just look for another GZIP header or EOF.
    // CarveSignature {
    //     file_type: "gzip",
    //     header: b"\x1F\x8B\x08",
    //     footer: b"",
    //     max_extent: 200 * 1024 * 1024,
    // },
];

// ── Public API ─────────────────────────────────────────────────────────────

/// Scan a byte stream for known file headers and carve out matching regions.
///
/// The reader must support `Seek` and `Read`. The function scans the entire
/// stream for known magic bytes, then from each header location, searches
/// forward for the footer. If no footer is found within `max_extent`, the
/// file is truncated at the limit.
///
/// This is a header/footer carver — it does not perform block-level carving
/// or file-system-aware recovery.
pub fn carve_from_unallocated<R: Read + Seek>(
    reader: &mut R,
    _fs_info: &str,
) -> io::Result<Vec<CarvedFile>> {
    let mut carved = Vec::new();

    // Determine total stream length.
    let total_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;

    if total_len < 4 {
        return Ok(carved);
    }

    // Read the entire stream into memory for scanning.
    // Cap at a reasonable upper bound to avoid OOM.
    const MAX_SCAN_SIZE: u64 = 256 * 1024 * 1024; // 256 MB
    let scan_len = total_len.min(MAX_SCAN_SIZE);
    let mut buffer = vec![0u8; scan_len as usize];
    reader.read_exact(&mut buffer)?;

    // Scan for each signature type.
    for sig in SIGNATURES {
        let mut search_start = 0usize;
        while let Some(header_pos) = find_magic(&buffer[search_start..], sig.header) {
            let abs_offset = search_start + header_pos;

            // Skip if this region overlaps an already-carved file.
            let overlap = carved.iter().any(|c: &CarvedFile| {
                let c_start = c.offset as usize;
                let c_end = c_start.saturating_add(c.size as usize);
                abs_offset >= c_start && abs_offset < c_end
            });
            if overlap {
                search_start = abs_offset + 1;
                continue;
            }

            // Search for footer.
            let footer_search_start = abs_offset.saturating_add(sig.header.len());
            let footer_search_end = (abs_offset as u64)
                .saturating_add(sig.max_extent)
                .min(buffer.len() as u64) as usize;

            let mut confidence = 0.5; // default: header found, footer uncertain
            let mut end_offset = abs_offset.saturating_add(sig.header.len());

            if !sig.footer.is_empty() && footer_search_start < footer_search_end {
                if let Some(footer_pos_offset) =
                    find_magic(&buffer[footer_search_start..footer_search_end], sig.footer)
                {
                    end_offset = footer_search_start
                        .saturating_add(footer_pos_offset)
                        .saturating_add(sig.footer.len());
                    confidence = 1.0;
                } else {
                    // No footer found — truncate at max_extent.
                    end_offset = (abs_offset as u64)
                        .saturating_add(sig.max_extent)
                        .min(buffer.len() as u64) as usize;
                    confidence = 0.3;
                }
            }

            let size = (end_offset - abs_offset) as u64;
            if size == 0 {
                search_start = abs_offset + 1;
                continue;
            }

            let data = buffer[abs_offset..end_offset].to_vec();
            carved.push(CarvedFile {
                file_type: sig.file_type.to_string(),
                offset: abs_offset as u64,
                size,
                data,
                confidence,
            });

            // Advance search past the carved region.
            search_start = end_offset;
        }
    }

    // Sort by offset for deterministic output.
    carved.sort_by_key(|c| c.offset);

    Ok(carved)
}

// ── Internal helpers ───────────────────────────────────────────────────────

/// Find the first occurrence of `needle` in `haystack` and return its byte
/// offset. Returns `None` if not found.
///
/// Uses a simple Boyer-Moore-Horspool-like search for clarity and correctness.
fn find_magic(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../tests/unit/file_carving.rs"]
mod tests;
