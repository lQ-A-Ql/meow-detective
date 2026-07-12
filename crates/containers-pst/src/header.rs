//! PST header parsing and low-level format constants.
//!
//! This module contains the magic bytes, page constants, BTree signatures,
//! header/root offsets, and the core block-reference and header types used
//! across the PST parser.

use crate::props::{read_u16_le, read_u32_le, read_u64_le};
use crate::PstError;

/// Magic bytes at the start of every PST file: "!BDN".
pub(crate) const PST_MAGIC: [u8; 4] = [0x21, 0x42, 0x44, 0x4E];

/// Page size in bytes.
pub(crate) const PAGE_SIZE: usize = 512;

/// Header size is always one page.
pub(crate) const HEADER_SIZE: usize = PAGE_SIZE;

/// Offset within the header where the ROOT structure begins (Unicode).
pub(crate) const HEADER_ROOT_OFFSET_UNICODE: usize = 188;

/// Offset within the header where the ROOT structure begins (ANSI).
pub(crate) const HEADER_ROOT_OFFSET_ANSI: usize = 176;

/// BTree page signature byte for NBT internal nodes.
pub(crate) const BTREE_INTERNAL: u8 = 0x02;

/// BTree page signature byte for NBT leaf nodes.
pub(crate) const BTREE_LEAF: u8 = 0x01;

/// BTree page signature (wSig field) for block BTree pages.
pub(crate) const BTREE_BB: u8 = 0x80;

/// NID for the root folder.
pub(crate) const NID_ROOT_FOLDER: u32 = 0x122;

/// NID for the search root folder.
pub(crate) const NID_SEARCH_ROOT: u32 = 0x61;

/// NID for the top of personal folders.
pub(crate) const NID_TOP_OF_PERSONAL_FOLDERS: u32 = 0x214;

/// A Block Reference points to a location in the PST file.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bref {
    /// Block ID.
    pub(crate) bid: u64,
    /// Byte index within the block.
    pub(crate) ib: u64,
}

impl Bref {
    /// Read a BREF from raw bytes at the given offset.
    /// For Unicode PST, a BREF is 16 bytes (bid:8 + ib:8).
    /// For ANSI PST, a BREF is 8 bytes (bid:4 + ib:4).
    pub(crate) fn from_bytes_unicode(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bid: read_u64_le(data, offset)?,
            ib: read_u64_le(data, offset + 8)?,
        })
    }

    pub(crate) fn from_bytes_ansi(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bid: read_u32_le(data, offset)? as u64,
            ib: read_u32_le(data, offset + 4)? as u64,
        })
    }
}

/// Parsed PST header information.
#[derive(Debug, Clone)]
pub(crate) struct PstHeader {
    /// True if this is a Unicode (64-bit) PST.
    pub(crate) is_unicode: bool,
    /// File size in bytes (ibFileEof from ROOT).
    pub(crate) file_size: u64,
    /// BREF pointing to the root of the Node BTree.
    pub(crate) root_nbt: Bref,
    /// BREF pointing to the root of the Block BTree.
    pub(crate) root_bbt: Bref,
}

/// An entry in the Node BTree.
#[derive(Debug, Clone)]
pub(crate) struct NbtEntry {
    pub(crate) nid: u32,
    pub(crate) bid_data: u64,
    pub(crate) bid_sub: u64,
}

impl NbtEntry {
    pub(crate) fn from_bytes_unicode(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            nid: read_u32_le(data, offset)?,
            bid_data: read_u64_le(data, offset + 8)?,
            bid_sub: read_u64_le(data, offset + 16)?,
        })
    }

    pub(crate) fn from_bytes_ansi(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            nid: read_u32_le(data, offset)?,
            bid_data: read_u32_le(data, offset + 4)? as u64,
            bid_sub: read_u32_le(data, offset + 8)? as u64,
        })
    }
}

/// An entry in the Block BTree.
///
/// The on-disk entry also contains `cb` (block size) and `c_ref` (reference
/// count) fields, but the reader only needs the block reference.
#[derive(Debug, Clone)]
pub(crate) struct BbtEntry {
    pub(crate) bref: Bref,
}

impl BbtEntry {
    pub(crate) fn from_bytes_unicode(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bref: Bref::from_bytes_unicode(data, offset)?,
        })
    }

    pub(crate) fn from_bytes_ansi(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bref: Bref::from_bytes_ansi(data, offset)?,
        })
    }
}

/// Parse the PST file header from the first 512 bytes.
pub(crate) fn parse_header(data: &[u8]) -> Result<PstHeader, PstError> {
    if data.len() < 512 {
        return Err(PstError::InvalidFormat(
            "File is too small to contain a PST header".to_string(),
        ));
    }

    // Verify magic bytes.
    let magic = &data[0..4];
    if magic != PST_MAGIC {
        return Err(PstError::InvalidFormat(format!(
            "Invalid PST magic bytes: expected {:02X?}, got {:02X?}",
            &PST_MAGIC[..],
            magic
        )));
    }

    // Read version at offset 10.
    let version = read_u16_le(data, 10).ok_or(PstError::InvalidFormat(
        "Failed to read PST version".to_string(),
    ))?;

    let is_unicode = version >= 23;

    // Parse ROOT structure.
    let root_offset = if is_unicode {
        HEADER_ROOT_OFFSET_UNICODE
    } else {
        HEADER_ROOT_OFFSET_ANSI
    };

    // Root layout for Unicode:
    //   +0: dwReserved (4)
    //   +4: ibFileEof (8)
    //  +12: ibAMapLast (8)
    //  +20: cbAMapFree (8)
    //  +28: cbPMapFree (8)
    //  +36: brefNBT (16)
    //  +52: dwAlign (4)
    //  +56: brefBBT (16)
    //
    // Root layout for ANSI:
    //   +0: dwReserved (4)
    //   +4: ibFileEof (4)
    //   +8: ibAMapLast (4)
    //  +12: cbAMapFree (4)
    //  +16: cbPMapFree (4)
    //  +20: brefNBT (8)
    //  +28: brefNBT2 (4)
    //  +32: brefBBT (8)

    let (file_size, root_nbt, root_bbt) = if is_unicode {
        let file_size = read_u64_le(data, root_offset + 4).unwrap_or(0);
        let nbt = Bref::from_bytes_unicode(data, root_offset + 36).ok_or(
            PstError::InvalidFormat("Failed to read NBT root BREF".to_string()),
        )?;
        let bbt = Bref::from_bytes_unicode(data, root_offset + 56).ok_or(
            PstError::InvalidFormat("Failed to read BBT root BREF".to_string()),
        )?;
        (file_size, nbt, bbt)
    } else {
        let file_size = read_u32_le(data, root_offset + 4).unwrap_or(0) as u64;
        let nbt = Bref::from_bytes_ansi(data, root_offset + 20).ok_or(PstError::InvalidFormat(
            "Failed to read NBT root BREF".to_string(),
        ))?;
        let bbt = Bref::from_bytes_ansi(data, root_offset + 32).ok_or(PstError::InvalidFormat(
            "Failed to read BBT root BREF".to_string(),
        ))?;
        (file_size, nbt, bbt)
    };

    Ok(PstHeader {
        is_unicode,
        file_size,
        root_nbt,
        root_bbt,
    })
}
