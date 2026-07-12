//! NTFS Alternate Data Streams (ADS).
//!
//! NTFS files can have multiple `$DATA` attributes. The first (unnamed)
//! attribute is the default stream; additional named `$DATA` attributes
//! are "alternate data streams" visible as `filename:streamname`.
//!
//! This module provides:
//! - `list_alternate_streams` — enumerate named `$DATA` attributes
//! - `read_alternate_stream` — read a single named stream

use crate::NtfsReader;
use std::io;

/// A single alternate data stream entry.
#[derive(Debug, Clone)]
pub struct AdsEntry {
    /// Name of the stream (e.g. "Zone.Identifier").
    pub name: String,
    /// Logical size of the stream content in bytes.
    pub size: u64,
    /// Allocated size of the stream on disk (cluster-aligned).
    pub allocated_size: u64,
}

/// Enumerate alternate (named) data streams for the file at `path`.
///
/// Returns an empty `Vec` if the path does not exist, is a directory, or
/// has no named $DATA attributes.
pub fn list_alternate_streams(reader: &NtfsReader, path: &str) -> io::Result<Vec<AdsEntry>> {
    let inode = match reader.resolve_file_path(path)? {
        Some(inode) => inode,
        None => return Ok(Vec::new()),
    };
    reader.list_ads_by_inode(inode)
}

/// Read the content of a single alternate data stream.
///
/// Returns an empty `Vec` if the stream is not found.
pub fn read_alternate_stream(
    reader: &NtfsReader,
    path: &str,
    stream_name: &str,
) -> io::Result<Vec<u8>> {
    let inode = match reader.resolve_file_path(path)? {
        Some(inode) => inode,
        None => return Ok(Vec::new()),
    };
    reader.read_ads_by_inode(inode, stream_name)
}

// ── ADS parsing helpers used by the main NtfsReader ──────────────────────

impl NtfsReader {
    /// Extract the UTF-16LE name from a named attribute header.
    pub(crate) fn read_attr_name(record: &[u8], attr_pos: usize) -> Option<String> {
        let name_len = *record.get(attr_pos + 0x09)? as usize;
        if name_len == 0 {
            return None;
        }
        let name_off = u16::from_le_bytes(
            record
                .get(attr_pos + 0x0A..attr_pos + 0x0C)?
                .try_into()
                .ok()?,
        ) as usize;
        let name_start = attr_pos.checked_add(name_off)?;
        let name_end = name_start.checked_add(name_len * 2)?;
        if name_end > record.len() {
            return None;
        }
        let chars: Vec<u16> = record[name_start..name_end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(String::from_utf16_lossy(&chars))
    }

    /// List all alternate (named) $DATA streams for a file by MFT inode.
    pub fn list_ads_by_inode(&self, inode: u64) -> io::Result<Vec<AdsEntry>> {
        let rec = self.read_mft_record(inode)?;
        if rec.len() < 0x18 || &rec[0..4] != b"FILE" {
            return Ok(Vec::new());
        }
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        let mut streams = Vec::new();

        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFFFFFF {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }

            if typ == 0x80 {
                // Only collect named $DATA attributes
                if let Some(name) = Self::read_attr_name(&rec, pos) {
                    let is_nonresident = pos + 9 <= rec.len() && (rec[pos + 8] & 1) != 0;
                    let (size, allocated_size) = if is_nonresident {
                        if pos + 0x38 > rec.len() {
                            (0, 0)
                        } else {
                            let real_size = u64::from_le_bytes(
                                rec[pos + 0x30..pos + 0x38].try_into().unwrap_or([0; 8]),
                            );
                            let alloc_size = u64::from_le_bytes(
                                rec[pos + 0x28..pos + 0x30].try_into().unwrap_or([0; 8]),
                            );
                            (real_size, alloc_size)
                        }
                    } else {
                        if pos + 0x14 > rec.len() {
                            (0, 0)
                        } else {
                            let content_size = u32::from_le_bytes(
                                rec[pos + 0x10..pos + 0x14].try_into().unwrap_or([0; 4]),
                            ) as u64;
                            (content_size, content_size)
                        }
                    };
                    streams.push(AdsEntry {
                        name,
                        size,
                        allocated_size,
                    });
                }
            }
            pos += len;
        }
        Ok(streams)
    }

    /// Read the content of a named $DATA stream by MFT inode and stream name.
    pub fn read_ads_by_inode(&self, inode: u64, stream_name: &str) -> io::Result<Vec<u8>> {
        let rec = self.read_mft_record(inode)?;
        if rec.len() < 0x18 || &rec[0..4] != b"FILE" {
            return Ok(Vec::new());
        }
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;

        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFFFFFF {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }

            if typ == 0x80 {
                if let Some(name) = Self::read_attr_name(&rec, pos) {
                    if name.eq_ignore_ascii_case(stream_name) {
                        let is_nonresident = pos + 9 <= rec.len() && (rec[pos + 8] & 1) != 0;
                        if is_nonresident {
                            if pos + 0x40 > rec.len() {
                                return Ok(Vec::new());
                            }
                            return self.read_attr_nonresident(pos, &rec);
                        } else {
                            if pos + 0x16 > rec.len() {
                                return Ok(Vec::new());
                            }
                            let content_size = u32::from_le_bytes(
                                rec[pos + 0x10..pos + 0x14].try_into().unwrap_or([0; 4]),
                            ) as usize;
                            let content_off = pos
                                + u16::from_le_bytes(
                                    rec[pos + 0x14..pos + 0x16].try_into().unwrap_or([0; 2]),
                                ) as usize;
                            let end = content_off.saturating_add(content_size).min(rec.len());
                            if content_off < end {
                                return Ok(rec[content_off..end].to_vec());
                            }
                            return Ok(Vec::new());
                        }
                    }
                }
            }
            pos += len;
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
#[path = "../tests/unit/ads.rs"]
mod tests;
