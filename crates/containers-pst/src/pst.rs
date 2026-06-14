//! PST (Personal Storage Table) format support.
//!
//! PST is the file format used by Microsoft Outlook to store email messages,
//! calendar entries, contacts, and other items locally.
//!
//! ## Format overview
//!
//! The PST format consists of:
//! - **NDB (Node Database)**: Fixed-size pages (512 bytes). The first page is
//!   the header, which contains pointers to the Node BTree (NBT) and Block
//!   BTree (BBT) roots.
//! - **NBT (Node BTree)**: Maps Node IDs (NIDs) to data block locations.
//!   Internal nodes contain `(nid, bidData, bidSub)` entries; leaf nodes
//!   omit `bidSub`.
//! - **BBT (Block BTree)**: Maps Block IDs (BIDs) to file byte offsets.
//! - **LTP (List, Table, Properties)**: Heap-on-Node and BTree-on-Heap
//!   structures for property storage.
//!
//! ## Supported formats
//!
//! - Unicode PST (64-bit, wVer=23)
//! - ANSI PST (32-bit, wVer=14/15)
//!
//! ## Known property IDs
//!
//! | ID     | Name                     |
//! |--------|--------------------------|
//! | 0x0037 | PidTagSubject            |
//! | 0x1000 | PidTagBody               |
//! | 0x0C1A | PidTagSenderName         |
//! | 0x0E1F | PidTagSenderEmailAddress |
//! | 0x0039 | PidTagClientSubmitTime   |
//! | 0x0E06 | PidTagMessageDeliveryTime|
//! | 0x0E04 | PidTagDisplayTo          |
//! | 0x0E03 | PidTagDisplayCc          |
//! | 0x0E17 | PidTagMessageClass       |
//! | 0x3701 | PidTagAttachmentData     |

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};

use crate::{PstAttachment, PstCalendar, PstContact, PstError, PstFolder, PstMessage};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Magic bytes at the start of every PST file: "!BDN".
const PST_MAGIC: [u8; 4] = [0x21, 0x42, 0x44, 0x4E];

/// Page size in bytes.
const PAGE_SIZE: usize = 512;

/// First NID in the `rgnid` array that typically gives us the root folder.
#[allow(dead_code)]
const HEADER_NID_INDEX_ROOT_FOLDER: usize = 0;

/// Header size is always one page.
const HEADER_SIZE: usize = PAGE_SIZE;

/// Offset within the header where the ROOT structure begins (Unicode).
const HEADER_ROOT_OFFSET_UNICODE: usize = 188;

/// Offset within the header where the ROOT structure begins (ANSI).
const HEADER_ROOT_OFFSET_ANSI: usize = 176;

// ─── Property tags ──────────────────────────────────────────────────────────

/// Property tag: subject.
const PROP_TAG_SUBJECT: u16 = 0x0037;
/// Property tag: body.
const PROP_TAG_BODY: u16 = 0x1000;
/// Property tag: sender name.
const PROP_TAG_SENDER_NAME: u16 = 0x0C1A;
/// Property tag: sender email address.
const PROP_TAG_SENDER_EMAIL: u16 = 0x0E1F;
/// Property tag: client submit time (sent time).
const PROP_TAG_SENT_TIME: u16 = 0x0039;
/// Property tag: message delivery time (received time).
const PROP_TAG_DELIVERY_TIME: u16 = 0x0E06;
/// Property tag: display to.
const PROP_TAG_DISPLAY_TO: u16 = 0x0E04;
/// Property tag: display cc.
const PROP_TAG_DISPLAY_CC: u16 = 0x0E03;
/// Property tag: message class.
const PROP_TAG_MESSAGE_CLASS: u16 = 0x001A;
/// Property tag: attachment binary data.
const PROP_TAG_ATTACH_DATA: u16 = 0x3701;
/// Property tag: attachment long filename.
const PROP_TAG_ATTACH_LONG_FILENAME: u16 = 0x3707;
/// Property tag: attachment mime type.
const PROP_TAG_ATTACH_MIME: u16 = 0x370E;
/// Property tag: attachment size.
const PROP_TAG_ATTACH_SIZE: u16 = 0x0E20;

// ─── Known NIDs ─────────────────────────────────────────────────────────────

/// NID for the message store.
#[allow(dead_code)]
const NID_MESSAGE_STORE: u32 = 0x21;
/// NID for the root folder.
const NID_ROOT_FOLDER: u32 = 0x122;
/// NID for the search root folder.
const NID_SEARCH_ROOT: u32 = 0x61;
/// NID for the top of personal folders.
const NID_TOP_OF_PERSONAL_FOLDERS: u32 = 0x214;

// ─── Property types ─────────────────────────────────────────────────────────

/// MAPI property type codes.
#[allow(non_upper_case_globals, dead_code)]
mod prop_type {
    pub const PtypInteger16: u16 = 0x0002;
    pub const PtypInteger32: u16 = 0x0003;
    pub const PtypFloating32: u16 = 0x0004;
    pub const PtypFloating64: u16 = 0x0005;
    pub const PtypBoolean: u16 = 0x000B;
    pub const PtypInteger64: u16 = 0x0014;
    pub const PtypString: u16 = 0x001F;
    pub const PtypString8: u16 = 0x001E;
    pub const PtypTime: u16 = 0x0040;
    pub const PtypBinary: u16 = 0x0102;
    pub const PtypMultipleInteger16: u16 = 0x1002;
    pub const PtypMultipleInteger32: u16 = 0x1003;
    pub const PtypMultipleString: u16 = 0x101F;
    pub const PtypMultipleBinary: u16 = 0x1102;
}

// ─── BTree page types ───────────────────────────────────────────────────────

/// BTree page signature byte for NBT internal nodes.
const BTREE_INTERNAL: u8 = 0x02;
/// BTree page signature byte for NBT leaf nodes.
const BTREE_LEAF: u8 = 0x01;

/// BTree page signature (wSig field) for block BTree pages.
const BTREE_BB: u8 = 0x80;
/// BTree page signature (wSig field) for node BTree pages.
#[allow(dead_code)]
const BTREE_NB: u8 = 0x81;

// ─── Helper types ───────────────────────────────────────────────────────────

/// A Block Reference points to a location in the PST file.
#[derive(Debug, Clone, Copy)]
struct Bref {
    /// Block ID.
    bid: u64,
    /// Byte index within the block.
    ib: u64,
}

impl Bref {
    /// Read a BREF from raw bytes at the given offset.
    /// For Unicode PST, a BREF is 16 bytes (bid:8 + ib:8).
    /// For ANSI PST, a BREF is 8 bytes (bid:4 + ib:4).
    fn from_bytes_unicode(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bid: read_u64_le(data, offset)?,
            ib: read_u64_le(data, offset + 8)?,
        })
    }
    fn from_bytes_ansi(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bid: read_u32_le(data, offset)? as u64,
            ib: read_u32_le(data, offset + 4)? as u64,
        })
    }
}

/// Parsed PST header information.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PstHeader {
    /// File format version: 14/15 = ANSI, 23 = Unicode.
    version: u16,
    /// True if this is a Unicode (64-bit) PST.
    is_unicode: bool,
    /// File size in bytes (ibFileEof from ROOT).
    file_size: u64,
    /// BREF pointing to the root of the Node BTree.
    root_nbt: Bref,
    /// BREF pointing to the root of the Block BTree.
    root_bbt: Bref,
}

/// An entry in the Node BTree.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct NbtEntry {
    nid: u32,
    bid_data: u64,
    bid_sub: u64,
}

impl NbtEntry {
    fn from_bytes_unicode(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            nid: read_u32_le(data, offset)?,
            bid_data: read_u64_le(data, offset + 8)?,
            bid_sub: read_u64_le(data, offset + 16)?,
        })
    }
    fn from_bytes_ansi(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            nid: read_u32_le(data, offset)?,
            bid_data: read_u32_le(data, offset + 4)? as u64,
            bid_sub: read_u32_le(data, offset + 8)? as u64,
        })
    }
}

/// An entry in the Block BTree.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct BbtEntry {
    bref: Bref,
    cb: u16,
    c_ref: u16,
}

impl BbtEntry {
    fn from_bytes_unicode(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bref: Bref::from_bytes_unicode(data, offset)?,
            cb: read_u16_le(data, offset + 16)?,
            c_ref: read_u16_le(data, offset + 18)?,
        })
    }
    fn from_bytes_ansi(data: &[u8], offset: usize) -> Option<Self> {
        Some(Self {
            bref: Bref::from_bytes_ansi(data, offset)?,
            cb: read_u16_le(data, offset + 8)?,
            c_ref: read_u16_le(data, offset + 10)?,
        })
    }
}

/// Represents a parsed property value.
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum PropValue {
    Null,
    I16(i16),
    I32(i32),
    I64(i64),
    Bool(bool),
    String(String),
    Binary(Vec<u8>),
    Filetime(Option<DateTime<Utc>>),
    StringArray(Vec<String>),
}

// ─── Byte-level read helpers ────────────────────────────────────────────────

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u64_le(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset + 8)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

fn read_f64_le(data: &[u8], offset: usize) -> Option<f64> {
    let bytes = data.get(offset..offset + 8)?;
    Some(f64::from_le_bytes(bytes.try_into().ok()?))
}

fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
    if ft == 0 || ft >= 0x8000000000000000 {
        return None;
    }
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
        .single()
}

// ─── PstReader ──────────────────────────────────────────────────────────────

/// A read-only PST file reader.
///
/// Opens a PST file, parses its header, and provides methods to extract
/// messages, folders, calendar entries, and contacts.
///
/// # Examples
///
/// ```ignore
/// let reader = PstReader::open("mailbox.pst").unwrap();
/// let messages = reader.read_messages().unwrap();
/// ```
pub struct PstReader {
    /// The entire PST file content loaded into memory.
    data: Vec<u8>,
    /// Parsed header information.
    header: PstHeader,
    /// Cached BBT entries keyed by BID for fast lookup.
    bbt_cache: BTreeMap<u64, BbtEntry>,
    /// Cached NBT entries keyed by NID.
    nbt_cache: BTreeMap<u32, NbtEntry>,
}

impl PstReader {
    /// Open a PST file from the given path.
    ///
    /// Reads the header, validates the magic bytes, detects Unicode vs ANSI,
    /// and caches the NBT and BBT entries for subsequent lookups.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PstError> {
        let data = fs::read(path)?;

        if data.len() < HEADER_SIZE {
            return Err(PstError::InvalidFormat(
                "File is too small to be a PST".to_string(),
            ));
        }

        let header = parse_header(&data)?;

        let mut reader = Self {
            data,
            header,
            bbt_cache: BTreeMap::new(),
            nbt_cache: BTreeMap::new(),
        };

        // Pre-cache BBT and NBT entries.
        reader.cache_bbt()?;
        reader.cache_nbt()?;

        Ok(reader)
    }

    /// Return whether this is a Unicode (64-bit) PST.
    pub fn is_unicode(&self) -> bool {
        self.header.is_unicode
    }

    /// Return the file size declared in the header.
    pub fn file_size(&self) -> u64 {
        self.header.file_size
    }

    // ── Public extraction methods ────────────────────────────────────────

    /// Extract all email messages from the PST.
    pub fn read_messages(&self) -> Result<Vec<PstMessage>, PstError> {
        let mut messages = Vec::new();
        let folder_nids = self.collect_folder_nids()?;

        for folder_nid in &folder_nids {
            let sub_nids = self.get_subnode_nids(*folder_nid)?;
            for sub_nid in sub_nids {
                let message_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                match message_class.as_deref() {
                    Some("IPM.Note")
                    | Some("IPM.Note.SMIME")
                    | Some("IPM.Note.SMIME.MultipartSigned") => {}
                    _ => {
                        // Skip non-mail items (appointments, contacts, etc.)
                        continue;
                    }
                }
                if let Ok(msg) = self.read_message(sub_nid, &self.get_folder_path(*folder_nid)) {
                    messages.push(msg);
                }
            }
        }

        Ok(messages)
    }

    /// Extract the folder hierarchy from the PST.
    pub fn read_folders(&self) -> Result<Vec<PstFolder>, PstError> {
        let mut folders = Vec::new();
        let nids = self.collect_folder_nids()?;

        for nid in &nids {
            let name = self
                .get_property_string(*nid, 0x3001) // PidTagDisplayName
                .unwrap_or_else(|| format!("Folder_{:X}", nid));
            let parent_path = self.get_folder_path(*nid);

            // Count children (messages/subfolders) via hierarchy table.
            let sub_nids = self.get_subnode_nids(*nid).unwrap_or_default();
            let message_count = sub_nids.len() as u64;

            // Count subfolders — approximate from NIDs that have this as parent.
            let subfolder_count = self.count_direct_subfolders(*nid);

            let depth = parent_path.split('/').filter(|s| !s.is_empty()).count() as u32;

            folders.push(PstFolder {
                name,
                parent_path,
                depth,
                message_count,
                subfolder_count,
            });
        }

        Ok(folders)
    }

    /// Extract calendar entries.
    pub fn read_calendar(&self) -> Result<Vec<PstCalendar>, PstError> {
        let mut calendars = Vec::new();
        let folder_nids = self.collect_folder_nids()?;

        for folder_nid in &folder_nids {
            let sub_nids = self.get_subnode_nids(*folder_nid)?;
            for sub_nid in sub_nids {
                let msg_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                if msg_class.as_deref() != Some("IPM.Appointment") {
                    continue;
                }

                let subject = self
                    .get_property_string(sub_nid, PROP_TAG_SUBJECT)
                    .unwrap_or_default();
                let location = self
                    .get_property_string(sub_nid, 0x8208) // PidTagLocation
                    .unwrap_or_default();

                let start_time = self.get_property_filetime(sub_nid, 0x820D); // PidLidAppointmentStartWhole
                let end_time = self.get_property_filetime(sub_nid, 0x820E); // PidLidAppointmentEndWhole

                let attendees = self
                    .get_property_string_array(sub_nid, 0x823E) // PidLidAttendeeString
                    .unwrap_or_default();

                calendars.push(PstCalendar {
                    subject,
                    start_time,
                    end_time,
                    location,
                    attendees,
                });
            }
        }

        Ok(calendars)
    }

    /// Extract contact entries.
    pub fn read_contacts(&self) -> Result<Vec<PstContact>, PstError> {
        let mut contacts = Vec::new();
        let folder_nids = self.collect_folder_nids()?;

        for folder_nid in &folder_nids {
            let sub_nids = self.get_subnode_nids(*folder_nid)?;
            for sub_nid in sub_nids {
                let msg_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                if msg_class.as_deref() != Some("IPM.Contact") {
                    continue;
                }

                let name = self
                    .get_property_string(sub_nid, 0x3A06) // PidTagDisplayName
                    .unwrap_or_default();

                let email = self
                    .get_property_string(sub_nid, 0x39FE) // PidTagPrimarySmtpAddress
                    .or_else(|| {
                        self.get_property_string(sub_nid, 0x8083) // PidTagEmailAddress
                    })
                    .unwrap_or_default();

                let phone = self
                    .get_property_string(sub_nid, 0x3A08) // PidTagBusinessTelephoneNumber
                    .or_else(|| {
                        self.get_property_string(sub_nid, 0x3A1C) // PidTagMobileTelephoneNumber
                    })
                    .unwrap_or_default();

                let address = self
                    .get_property_string(sub_nid, 0x3A29) // PidTagStreetAddress
                    .unwrap_or_default();

                contacts.push(PstContact {
                    name,
                    email,
                    phone,
                    address,
                });
            }
        }

        Ok(contacts)
    }

    // ── Internal: NBT/BBT caching ────────────────────────────────────────

    fn cache_bbt(&mut self) -> Result<(), PstError> {
        let bref = self.header.root_bbt;
        if bref.bid == 0 {
            return Ok(());
        }
        // Seed the cache with the root BREF so bid_to_file_offset can bootstrap.
        self.bbt_cache.insert(
            bref.bid,
            BbtEntry {
                bref,
                cb: 0,
                c_ref: 0,
            },
        );
        self.load_bbt_page(bref.bid)?;
        Ok(())
    }

    fn load_bbt_page(&mut self, bid: u64) -> Result<(), PstError> {
        let page_offset = self.bid_to_file_offset(bid);
        if page_offset + PAGE_SIZE > self.data.len() {
            return Err(PstError::InvalidFormat(format!(
                "BBT page BID 0x{:X} at offset {} is out of bounds",
                bid, page_offset
            )));
        }

        let page_data = &self.data[page_offset..page_offset + PAGE_SIZE];

        // Read page header to determine entry layout.
        let sig = page_data[0]; // first byte of wSig
        let level = page_data.get(22).copied().unwrap_or(0);
        let c_ent = page_data.get(23).copied().unwrap_or(0) as usize;
        let cb_ent = page_data.get(24).copied().unwrap_or(0) as usize;

        if sig == BTREE_LEAF || sig == BTREE_INTERNAL {
            // This is a BTree page with btypeBTC inside NBT — skip for BBT.
            // The BBT uses different sig bytes.
        }

        // BBT pages: wSig is typically 0x80 or 0xB580.
        let is_bbt_page =
            sig == BTREE_BB || sig == BTREE_LEAF || sig == BTREE_INTERNAL || sig == 0xB5; // btypeBTC

        if !is_bbt_page && sig != BTREE_LEAF && sig != BTREE_INTERNAL {
            // For raw/leaf BBT pages: try parsing entries.
        }

        // Use the entry layout from the page.
        let entries_offset = if self.header.is_unicode { 40 } else { 24 };

        for i in 0..c_ent {
            let ent_offset = entries_offset + i * cb_ent;
            let entry = if self.header.is_unicode {
                BbtEntry::from_bytes_unicode(page_data, ent_offset)
            } else {
                BbtEntry::from_bytes_ansi(page_data, ent_offset)
            };
            if let Some(entry) = entry {
                self.bbt_cache.insert(entry.bref.bid, entry);
            }
        }

        // Collect child BIDs first (avoid borrowing page_data across mutable self calls).
        let mut child_bids: Vec<u64> = Vec::new();
        if level > 0 {
            for i in 0..c_ent {
                let ent_offset = entries_offset + i * cb_ent;
                let child_bid = if self.header.is_unicode {
                    read_u64_le(page_data, ent_offset + 8).unwrap_or(0)
                } else {
                    read_u32_le(page_data, ent_offset + 4).unwrap_or(0) as u64
                };
                if child_bid != 0 && !self.bbt_cache.contains_key(&child_bid) {
                    let child_page = self.bid_to_file_offset(child_bid);
                    if child_page > 0 && child_page < self.data.len() {
                        child_bids.push(child_bid);
                    }
                }
            }
        }
        // Recurse into child pages (no longer borrowing page_data).
        for child_bid in child_bids {
            self.load_bbt_page(child_bid).ok();
        }

        Ok(())
    }

    fn cache_nbt(&mut self) -> Result<(), PstError> {
        let bid = self.header.root_nbt.bid;
        if bid == 0 {
            return Err(PstError::InvalidFormat(
                "No NBT root found in header".to_string(),
            ));
        }
        self.load_nbt_page(bid)?;
        Ok(())
    }

    fn load_nbt_page(&mut self, bid: u64) -> Result<(), PstError> {
        let page_offset = self.bid_to_file_offset(bid);
        if page_offset + PAGE_SIZE > self.data.len() {
            return Err(PstError::InvalidFormat(format!(
                "NBT page BID 0x{:X} at offset {} is out of bounds",
                bid, page_offset
            )));
        }

        let page_data = &self.data[page_offset..page_offset + PAGE_SIZE];

        let level = page_data.get(22).copied().unwrap_or(0);
        let c_ent = page_data.get(23).copied().unwrap_or(0) as usize;
        let cb_ent = page_data.get(24).copied().unwrap_or(0) as usize;

        let entries_offset = if self.header.is_unicode { 40 } else { 24 };

        for i in 0..c_ent {
            let ent_offset = entries_offset + i * cb_ent;
            let entry = if self.header.is_unicode {
                NbtEntry::from_bytes_unicode(page_data, ent_offset)
            } else {
                NbtEntry::from_bytes_ansi(page_data, ent_offset)
            };
            if let Some(entry) = entry {
                let nid = entry.nid;
                self.nbt_cache.insert(nid, entry);
            }
        }

        // Collect sub-node BIDs first (avoid borrowing page_data across mutable self calls).
        let mut sub_bids: Vec<u64> = Vec::new();
        if level > 0 {
            for i in 0..c_ent {
                let ent_offset = entries_offset + i * cb_ent;
                let sub_bid = if self.header.is_unicode {
                    read_u64_le(page_data, ent_offset + 16).unwrap_or(0)
                } else {
                    read_u32_le(page_data, ent_offset + 8).unwrap_or(0) as u64
                };
                if sub_bid != 0 {
                    sub_bids.push(sub_bid);
                }
            }
        }
        // Recurse into child nodes (no longer borrowing page_data).
        for sub_bid in sub_bids {
            self.load_nbt_page(sub_bid).ok();
        }

        Ok(())
    }

    // ── Internal: data access ────────────────────────────────────────────

    /// Convert a BID to a file byte offset.
    fn bid_to_file_offset(&self, bid: u64) -> usize {
        // First check BBT cache.
        if let Some(entry) = self.bbt_cache.get(&bid) {
            return entry.bref.ib as usize;
        }
        // Fallback: assume bid * page_size gives the file offset.
        (bid as usize) * PAGE_SIZE
    }

    /// Read the raw data block for a given NID.
    #[allow(dead_code)]
    fn read_data_block(&self, nid: u32) -> Option<&[u8]> {
        let entry = self.nbt_cache.get(&nid)?;
        let bid = entry.bid_data;
        if bid == 0 {
            return None;
        }
        let offset = self.bid_to_file_offset(bid);
        let end = (offset + PAGE_SIZE).min(self.data.len());
        Some(&self.data[offset..end])
    }

    /// Read the sub-node block for a given NID (the subnode BTree of properties).
    /// Falls back to the data block when no sub-node is present.
    fn read_subnode_block(&self, nid: u32) -> Option<&[u8]> {
        let entry = self.nbt_cache.get(&nid)?;
        let bid = if entry.bid_sub != 0 {
            entry.bid_sub
        } else {
            entry.bid_data
        };
        if bid == 0 {
            return None;
        }
        let offset = self.bid_to_file_offset(bid);
        let end = (offset + PAGE_SIZE).min(self.data.len());
        Some(&self.data[offset..end])
    }

    // ── Internal: property extraction ────────────────────────────────────

    /// Get a string property value for a given NID and property tag.
    fn get_property_string(&self, nid: u32, prop_tag: u16) -> Option<String> {
        let block = self.read_subnode_block(nid)?;
        self.find_prop_string(block, prop_tag)
    }

    /// Get a FILETIME property value.
    fn get_property_filetime(&self, nid: u32, prop_tag: u16) -> Option<DateTime<Utc>> {
        let block = self.read_subnode_block(nid)?;
        self.find_prop_filetime(block, prop_tag)
    }

    /// Get a multi-valued string property.
    fn get_property_string_array(&self, nid: u32, prop_tag: u16) -> Option<Vec<String>> {
        let block = self.read_subnode_block(nid)?;
        self.find_prop_string_array(block, prop_tag)
    }

    /// Parse a property context (Heap-on-Node) and return all properties.
    fn parse_property_context(&self, data: &[u8]) -> BTreeMap<u32, PropValue> {
        let mut props = BTreeMap::new();
        if data.len() < 16 {
            return props;
        }

        // Heap-on-Node header:
        // byte 0-1: ibHnpm (byte index of HN page map)
        // byte 2-3: bSig (0xEC = bTypeHN)
        // byte 4-5: bClientSig
        // byte 6-9: hidUserRoot
        // For our purposes, we mainly need to parse BTree-on-Heap or direct properties.

        // Try to find a property context by scanning for BTree-on-Heap signature.
        // Property context BTH header:
        // bType (1) = 0xB5
        // cbKey (1) = 2 or 4 (property tag size)
        // cbEnt (1) = size of each entry
        // bIdxLevels (1) = 0 (leaf)
        // hidRoot (4) = root HID

        // Scan for the "B5 02" or "B5 04" signatures that indicate a property BTH.
        for scan in 0..data.len().saturating_sub(8) {
            if data[scan] != 0xB5 {
                continue;
            }
            let cb_key = data[scan + 1];
            if cb_key != 2 && cb_key != 4 {
                continue;
            }
            let cb_ent = data[scan + 2];
            if cb_ent == 0 {
                continue;
            }
            let _b_idx_levels = data[scan + 3];
            let hid_root = read_u32_le(data, scan + 4).unwrap_or(0);
            if hid_root == 0 {
                continue;
            }

            // HID: high 5 bits = hidType (0 = HN block), low 27 bits = hidIndex.
            let hid_index = (hid_root & 0x07FF_FFFF) as usize;
            if hid_index >= data.len() {
                continue;
            }

            // Read the BTH entries starting from hid_index.
            // Each entry is cbEnt bytes: tag (cbKey bytes) + value (remaining).
            // For cbKey=2: tag is u16; for cbKey=4: tag is u32.
            // Value format depends on the property type encoded in the tag.
            let val_offset = cb_key as usize;
            let mut idx = hid_index;

            // Limit to a reasonable number of entries.
            for _ in 0..1000 {
                if idx + cb_ent as usize > data.len() {
                    break;
                }

                let tag = if cb_key == 2 {
                    match read_u16_le(data, idx) {
                        Some(v) => v as u32,
                        None => break,
                    }
                } else {
                    match read_u32_le(data, idx) {
                        Some(v) => v,
                        None => break,
                    }
                };

                let _prop_id = (tag >> 16) as u16;
                let prop_type = (tag & 0xFFFF) as u16;
                let full_tag = tag;

                let val_start = idx + val_offset;
                let val_data = &data[val_start..];

                if let Some(val) = self.read_prop_value(prop_type, val_data) {
                    props.insert(full_tag, val);
                }

                idx += cb_ent as usize;
            }

            break;
        }

        props
    }

    /// Read a property value of the given type from raw bytes.
    fn read_prop_value(&self, prop_type: u16, data: &[u8]) -> Option<PropValue> {
        match prop_type {
            prop_type::PtypInteger16 => Some(PropValue::I16(read_u16_le(data, 0)? as i16)),
            prop_type::PtypInteger32 => Some(PropValue::I32(read_u32_le(data, 0)? as i32)),
            prop_type::PtypInteger64 => Some(PropValue::I64(read_u64_le(data, 0)? as i64)),
            prop_type::PtypBoolean => {
                let v = data.first().copied().unwrap_or(0);
                Some(PropValue::Bool(v != 0))
            }
            prop_type::PtypFloating32 => {
                if data.len() >= 4 {
                    let bytes: [u8; 4] = data[..4].try_into().ok()?;
                    Some(PropValue::I32(f32::from_le_bytes(bytes) as i32))
                } else {
                    None
                }
            }
            prop_type::PtypFloating64 => {
                if data.len() >= 8 {
                    let v = read_f64_le(data, 0)?;
                    Some(PropValue::I64(v as i64))
                } else {
                    None
                }
            }
            prop_type::PtypString => {
                // Unicode string: length-prefixed in bytes, then UTF-16LE data.
                let len =
                    std::cmp::min(read_u32_le(data, 0)? as usize, data.len().saturating_sub(4));
                let str_data = data.get(4..4 + len)?;
                let chars: Vec<u16> = str_data
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .filter(|&c| c != 0) // skip null terminators
                    .collect();
                let s = String::from_utf16(&chars).ok()?;
                Some(PropValue::String(s))
            }
            prop_type::PtypString8 => {
                // ANSI string: length-prefixed, then ASCII/ANSI data.
                let len =
                    std::cmp::min(read_u32_le(data, 0)? as usize, data.len().saturating_sub(4));
                let str_data = data.get(4..4 + len)?;
                let s = String::from_utf8_lossy(str_data)
                    .trim_end_matches('\0')
                    .to_string();
                Some(PropValue::String(s))
            }
            prop_type::PtypTime => {
                let ft = read_u64_le(data, 0)?;
                Some(PropValue::Filetime(filetime_to_dt(ft)))
            }
            prop_type::PtypBinary => {
                let len =
                    std::cmp::min(read_u32_le(data, 0)? as usize, data.len().saturating_sub(4));
                let bin = data.get(4..4 + len)?.to_vec();
                Some(PropValue::Binary(bin))
            }
            prop_type::PtypMultipleString => {
                // Sequence of null-terminated UTF-16LE strings.
                let mut strings = Vec::new();
                let avail = data.len();
                let mut pos = 0;
                while pos + 2 <= avail {
                    let mut end = pos;
                    while end + 2 <= avail {
                        let w = u16::from_le_bytes([data[end], data[end + 1]]);
                        if w == 0 {
                            break;
                        }
                        end += 2;
                    }
                    if end > pos {
                        let chars: Vec<u16> = data[pos..end]
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        if let Ok(s) = String::from_utf16(&chars) {
                            if !s.is_empty() {
                                strings.push(s);
                            }
                        }
                    }
                    pos = end + 2;
                    if end + 2 >= avail {
                        break;
                    }
                }
                Some(PropValue::StringArray(strings))
            }
            _ => {
                // Unknown type — skip.
                None
            }
        }
    }

    /// Find a string property value in a property context block.
    fn find_prop_string(&self, data: &[u8], prop_id: u16) -> Option<String> {
        let props = self.parse_property_context(data);
        for full_tag in [
            prop_id as u32,
            ((prop_id as u32) << 16) | prop_type::PtypString as u32,
        ] {
            if let Some(PropValue::String(s)) = props.get(&full_tag) {
                return Some(s.clone());
            }
        }
        // Try with PtypString8 type.
        if let Some(PropValue::String(s)) =
            props.get(&(((prop_id as u32) << 16) | prop_type::PtypString8 as u32))
        {
            return Some(s.clone());
        }
        None
    }

    /// Find a FILETIME property value.
    fn find_prop_filetime(&self, data: &[u8], prop_id: u16) -> Option<DateTime<Utc>> {
        let props = self.parse_property_context(data);
        let tag = ((prop_id as u32) << 16) | prop_type::PtypTime as u32;
        if let Some(PropValue::Filetime(Some(dt))) = props.get(&tag) {
            return Some(*dt);
        }
        if let Some(PropValue::I64(ft)) = props.get(&((prop_id as u32) << 16)) {
            return filetime_to_dt(*ft as u64);
        }
        None
    }

    /// Find a multi-valued string property.
    fn find_prop_string_array(&self, data: &[u8], prop_id: u16) -> Option<Vec<String>> {
        let props = self.parse_property_context(data);
        let tag = ((prop_id as u32) << 16) | prop_type::PtypMultipleString as u32;
        if let Some(PropValue::StringArray(arr)) = props.get(&tag) {
            return Some(arr.clone());
        }
        None
    }

    // ── Internal: folder hierarchy ───────────────────────────────────────

    /// Collect all folder NIDs by traversing the hierarchy from the root folder.
    fn collect_folder_nids(&self) -> Result<Vec<u32>, PstError> {
        let mut folder_nids = Vec::new();

        // Start from well-known folder NIDs.
        let roots = [
            NID_ROOT_FOLDER,
            NID_TOP_OF_PERSONAL_FOLDERS,
            NID_SEARCH_ROOT,
        ];

        for root_nid in &roots {
            if self.nbt_cache.contains_key(root_nid) {
                folder_nids.push(*root_nid);
                self.collect_child_folders(*root_nid, &mut folder_nids);
            }
        }

        // If no well-known NIDs found, collect all NIDs that appear to be folders.
        if folder_nids.is_empty() {
            for (&nid, _entry) in self.nbt_cache.iter() {
                // Heuristic: folder NIDs tend to be in certain ranges.
                // Or check if the node has a sub-node (property context) with DisplayName.
                if self.read_subnode_block(nid).is_some() {
                    let msg_class = self.get_property_string(nid, PROP_TAG_MESSAGE_CLASS);
                    if msg_class.as_deref() == Some("IPM.Note")
                        || msg_class.as_deref() == Some("IPM.Appointment")
                        || msg_class.as_deref() == Some("IPM.Contact")
                    {
                        // This is a message, not a folder. Skip.
                        continue;
                    }
                    folder_nids.push(nid);
                }
            }
        }

        Ok(folder_nids)
    }

    fn collect_child_folders(&self, parent_nid: u32, result: &mut Vec<u32>) {
        // Try to read sub-node NIDs from hierarchy table.
        if let Ok(sub_nids) = self.get_subnode_nids(parent_nid) {
            for sub_nid in sub_nids {
                let msg_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                match msg_class.as_deref() {
                    Some("IPM.Note")
                    | Some("IPM.Note.SMIME")
                    | Some("IPM.Appointment")
                    | Some("IPM.Contact") => continue,
                    _ => {}
                }
                // Check if this is a folder by looking for folder-specific properties.
                if self.get_property_string(sub_nid, 0x3613).is_some()
                // PidTagContainerClass
                {
                    result.push(sub_nid);
                    self.collect_child_folders(sub_nid, result);
                }
            }
        }

        // Also walk NBT entries that might be children of this folder.
        for (&nid, _entry) in self.nbt_cache.iter() {
            if nid == parent_nid {
                continue;
            }
            // Check if this NID's parent relationship points to parent_nid.
            if let Some(_block) = self.read_subnode_block(nid) {
                // PidTagParentEntryId = 0x0E09
                // We can't easily check this without decoding EntryID.
                // Skip for now.
            }
        }
    }

    /// Retrieve sub-node NIDs (messages/items) for a given folder NID.
    fn get_subnode_nids(&self, folder_nid: u32) -> Result<Vec<u32>, PstError> {
        let mut sub_nids = Vec::new();

        // Find NBT entries whose parent is this folder.
        // This is determined by looking at the sub-node BTree of the folder.
        let _block = match self.read_subnode_block(folder_nid) {
            Some(b) => b,
            None => return Ok(sub_nids),
        };

        // Look for a contents table — a BTree-on-Heap that lists items.
        // The contents table has rows, each row has a NID.
        // For now, collect all NIDs from the NBT that have bid_data but no bid_sub.
        for (&nid, entry) in self.nbt_cache.iter() {
            if nid == folder_nid {
                continue;
            }
            // Heuristic: leaf entries have bid_data but bid_sub == 0.
            // But actually, leaf NBT entries ARE the messages' data nodes.
            // In the NBT, each message NID has a bidData pointing to its data block.
            // The NBT itself doesn't encode parent/child relationships for items.

            // A better heuristic: if the nid has a sub-node block (has properties),
            // it's an item. Parse its properties to see if it's a message.
            if entry.bid_sub != 0 {
                if let Some(sub_block) = self.read_subnode_block(nid) {
                    let props = self.parse_property_context(sub_block);
                    // Check for message-class related properties.
                    // Actually, let's just collect all NIDs with sub-blocks.
                    if !props.is_empty() {
                        sub_nids.push(nid);
                    }
                }
            }
        }

        Ok(sub_nids)
    }

    /// Count subfolders that are direct children of the given folder NID.
    fn count_direct_subfolders(&self, _parent_nid: u32) -> u64 {
        // Approximate: walk all folder NIDs and check hierarchy.
        // For the MVP, return 0 (the caller can improve this).
        0
    }

    /// Build a folder path string like "/Inbox/Subfolder".
    fn get_folder_path(&self, nid: u32) -> String {
        let name = self
            .get_property_string(nid, 0x3001)
            .unwrap_or_else(|| format!("Folder_{:X}", nid));
        format!("/{}", name)
    }

    /// Read a single message from the given NID.
    fn read_message(&self, nid: u32, folder_path: &str) -> Result<PstMessage, PstError> {
        let block = self
            .read_subnode_block(nid)
            .ok_or_else(|| PstError::InvalidFormat(format!("No data block for NID {:X}", nid)))?;

        let _props = self.parse_property_context(block);

        let subject = self
            .get_property_string(nid, PROP_TAG_SUBJECT)
            .unwrap_or_default();
        let body_plain = self
            .get_property_string(nid, PROP_TAG_BODY)
            .unwrap_or_default();
        let body_html = self
            .get_property_string(nid, 0x1013) // PidTagHtml
            .unwrap_or_default();
        let sender_name = self
            .get_property_string(nid, PROP_TAG_SENDER_NAME)
            .unwrap_or_default();
        let sender_email = self
            .get_property_string(nid, PROP_TAG_SENDER_EMAIL)
            .unwrap_or_default();
        let sent_time = self.get_property_filetime(nid, PROP_TAG_SENT_TIME);
        let received_time = self.get_property_filetime(nid, PROP_TAG_DELIVERY_TIME);

        let to = self
            .get_property_string(nid, PROP_TAG_DISPLAY_TO)
            .unwrap_or_default();
        let cc = self
            .get_property_string(nid, PROP_TAG_DISPLAY_CC)
            .unwrap_or_default();

        let mut recipients: Vec<String> = Vec::new();
        if !to.is_empty() {
            for addr in to.split(';') {
                let trimmed = addr.trim();
                if !trimmed.is_empty() {
                    recipients.push(trimmed.to_string());
                }
            }
        }
        if !cc.is_empty() {
            for addr in cc.split(';') {
                let trimmed = addr.trim();
                if !trimmed.is_empty() {
                    recipients.push(trimmed.to_string());
                }
            }
        }

        // Extract attachments from sub-node table.
        let attachments = self.read_attachments(nid);

        Ok(PstMessage {
            subject,
            body_plain,
            body_html,
            sender_name,
            sender_email,
            recipients,
            sent_time,
            received_time,
            attachments,
            folder_path: folder_path.to_string(),
        })
    }

    /// Extract attachments for a message NID.
    fn read_attachments(&self, message_nid: u32) -> Vec<PstAttachment> {
        let mut attachments = Vec::new();

        // Attachments are stored as sub-entries in the message's sub-node BTree.
        // Each attachment has NIDs in the range [message_nid + 1, message_nid + N].
        // Try to find attachment NIDs by scanning the NBT cache.
        for (&nid, _entry) in self.nbt_cache.iter() {
            // Attachment NIDs are typically close to the message NID.
            if nid <= message_nid || nid > message_nid + 1000 {
                continue;
            }

            // Check the MessageClass to see if it's an attachment.
            let msg_class = self.get_property_string(nid, PROP_TAG_MESSAGE_CLASS);
            if msg_class.as_deref() != Some("IPM.Attachment") {
                continue;
            }

            let name = self
                .get_property_string(nid, PROP_TAG_ATTACH_LONG_FILENAME)
                .or_else(|| {
                    self.get_property_string(nid, 0x3704) // PidTagAttachFilename
                })
                .unwrap_or_else(|| "unnamed".to_string());

            let attach_size_str = self.get_property_string(nid, PROP_TAG_ATTACH_SIZE);
            let size = attach_size_str
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            let mime_type = self
                .get_property_string(nid, PROP_TAG_ATTACH_MIME)
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let content_id = self.get_property_string(nid, 0x3712); // PidTagAttachContentId

            // Try to read binary attachment data.
            let data = self
                .get_property_binary(nid, PROP_TAG_ATTACH_DATA)
                .unwrap_or_default();

            attachments.push(PstAttachment {
                name,
                size,
                content_id,
                mime_type,
                data,
            });
        }

        attachments
    }

    /// Get a binary property value.
    fn get_property_binary(&self, nid: u32, prop_tag: u16) -> Option<Vec<u8>> {
        let block = self.read_subnode_block(nid)?;
        let props = self.parse_property_context(block);
        let tag = ((prop_tag as u32) << 16) | prop_type::PtypBinary as u32;
        if let Some(PropValue::Binary(data)) = props.get(&tag) {
            return Some(data.clone());
        }
        None
    }
}

// ─── Header parsing ─────────────────────────────────────────────────────────

fn parse_header(data: &[u8]) -> Result<PstHeader, PstError> {
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
        version,
        is_unicode,
        file_size,
        root_nbt,
        root_bbt,
    })
}

// ─── Synthetic fixture builder (module scope) ──────────────────────────────

/// Build a minimal synthetic Unicode PST for testing.
///
/// Produces a valid 512-byte-aligned in-memory PST with header, NBT, BBT,
/// and property context pages. Used by both PST and OST test suites.
#[doc(hidden)]
pub fn build_synthetic_pst() -> Vec<u8> {
    build_synthetic_unicode_pst()
}

/// Build the full synthetic Unicode PST fixture (module-private).
fn build_synthetic_unicode_pst() -> Vec<u8> {
    let mut pst = vec![0u8; 512 * 8]; // 8 pages

    // ═══ PAGE 0: Header (512 bytes) ═══
    let header = &mut pst[0..512];

    // Magic: "!BDN"
    header[0..4].copy_from_slice(&PST_MAGIC);
    // dwCRCPartial (4-7): zero
    // wMagicClient (8-9): "SM"
    header[8] = 0x53; // 'S'
    header[9] = 0x4D; // 'M'
                      // wVer (10-11): 23 = Unicode (64-bit)
    header[10] = 23u8;
    header[11] = 0u8;
    // wVerClient (12-13): 19
    header[12] = 19u8;
    header[13] = 0u8;
    // bPlatformCreate (14): 1
    header[14] = 1u8;
    // bPlatformAccess (15): 1
    header[15] = 1u8;

    // ROOT at offset 188 (Unicode):
    let file_size = (512 * 8) as u64;
    header[188 + 4..188 + 12].copy_from_slice(&file_size.to_le_bytes());
    // brefNBT (bid=4, ib=2048)
    let nbt_bid: u64 = 4;
    let nbt_ib: u64 = 512 * 4;
    header[188 + 36..188 + 44].copy_from_slice(&nbt_bid.to_le_bytes());
    header[188 + 44..188 + 52].copy_from_slice(&nbt_ib.to_le_bytes());
    // brefBBT (bid=2, ib=1024)
    let bbt_bid: u64 = 2;
    let bbt_ib: u64 = 512 * 2;
    header[188 + 56..188 + 64].copy_from_slice(&bbt_bid.to_le_bytes());
    header[188 + 64..188 + 72].copy_from_slice(&bbt_ib.to_le_bytes());

    // ═══ PAGE 2: BBT leaf page (at offset 1024) ═══
    let bbt_page = &mut pst[1024..1536];
    bbt_page[0] = BTREE_BB;
    bbt_page[1] = 0x00;
    bbt_page[2] = 0xEC;
    bbt_page[3] = 1;
    let _ = bid_to_bytes(2, &mut bbt_page[8..16]);
    bbt_page[22] = 0u8; // leaf level
    bbt_page[23] = 6u8; // 6 entries

    let cb_ent: u16 = 24; // Unicode BBT entry: bref(16) + cb(2) + cRef(2) + alignment(4)
    bbt_page[24] = cb_ent as u8;
    bbt_page[25] = (cb_ent >> 8) as u8;

    let bbt_entries: [(u64, u64, u16); 6] = [
        (1, 0, 1),
        (2, 1024, 1),
        (3, 1536, 1),
        (4, 2048, 1),
        (5, 2560, 1),
        (6, 3072, 1),
    ];
    for (i, (bid_val, ib_val, c_ref)) in bbt_entries.iter().enumerate() {
        let offset = 40 + i * cb_ent as usize;
        bbt_page[offset..offset + 8].copy_from_slice(&bid_val.to_le_bytes());
        bbt_page[offset + 8..offset + 16].copy_from_slice(&ib_val.to_le_bytes());
        bbt_page[offset + 16..offset + 18].copy_from_slice(&0u16.to_le_bytes()); // cb = 0
        bbt_page[offset + 18..offset + 20].copy_from_slice(&c_ref.to_le_bytes());
    }

    // ═══ PAGE 4: NBT root page (leaf) at offset 2048 ═══
    let nbt_page = &mut pst[2048..2560];
    nbt_page[0] = BTREE_NB;
    nbt_page[1] = 0x00;
    nbt_page[2] = 0xEC;
    nbt_page[3] = 1;
    let _ = bid_to_bytes(4, &mut nbt_page[8..16]);
    nbt_page[22] = 0u8; // leaf level
    nbt_page[23] = 4u8; // 4 entries

    let nbt_ent_size: u16 = 24;
    nbt_page[24] = nbt_ent_size as u8;
    nbt_page[25] = (nbt_ent_size >> 8) as u8;

    let nbt_entries: [(u32, u64, u64); 4] = [
        (NID_MESSAGE_STORE, 5, 0),
        (NID_ROOT_FOLDER, 6, 0),
        (0x8001, 7, 0),
        (0x8021, 8, 0),
    ];
    for (i, (nid, bid_data, bid_sub)) in nbt_entries.iter().enumerate() {
        let offset = 40 + i * nbt_ent_size as usize;
        nbt_page[offset..offset + 4].copy_from_slice(&nid.to_le_bytes());
        nbt_page[offset + 8..offset + 16].copy_from_slice(&bid_data.to_le_bytes());
        nbt_page[offset + 16..offset + 24].copy_from_slice(&bid_sub.to_le_bytes());
    }

    // ═══ PAGE 5: Property context for message store (offset 2560) ═══
    let pc_page = &mut pst[2560..3072];
    let bth_offset: usize = 40;
    let data_offset = bth_offset + 8;
    let hid_root: u32 = data_offset as u32;
    pc_page[bth_offset] = 0xB5;
    pc_page[bth_offset + 1] = 2u8;
    pc_page[bth_offset + 2] = 12u8;
    pc_page[bth_offset + 3] = 0u8;
    pc_page[bth_offset + 4..bth_offset + 8].copy_from_slice(&hid_root.to_le_bytes());

    let tag_display_name: u32 = 0x3001_001F;
    pc_page[data_offset..data_offset + 4].copy_from_slice(&tag_display_name.to_le_bytes());
    let str_offset = data_offset + 12;
    let name = "MessageStore\0";
    let utf16_bytes: Vec<u8> = name.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let str_len = utf16_bytes.len() as u32;
    pc_page[data_offset + 4..data_offset + 8].copy_from_slice(&str_len.to_le_bytes());
    pc_page[str_offset..str_offset + utf16_bytes.len()].copy_from_slice(&utf16_bytes);

    // ═══ PAGE 6: Property context for root folder (offset 3072) ═══
    let pc_page6 = &mut pst[3072..3584];
    let bth_offset6: usize = 40;
    let data_offset6 = bth_offset6 + 8;
    let hid_root6: u32 = data_offset6 as u32;
    pc_page6[bth_offset6] = 0xB5;
    pc_page6[bth_offset6 + 1] = 2u8;
    pc_page6[bth_offset6 + 2] = 12u8;
    pc_page6[bth_offset6 + 3] = 0u8;
    pc_page6[bth_offset6 + 4..bth_offset6 + 8].copy_from_slice(&hid_root6.to_le_bytes());

    let tag_display: u32 = 0x3001_001F;
    pc_page6[data_offset6..data_offset6 + 4].copy_from_slice(&tag_display.to_le_bytes());
    let str_offset6 = data_offset6 + 12;
    let name6 = "RootFolder\0";
    let utf16_bytes6: Vec<u8> = name6.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let str_len6 = utf16_bytes6.len() as u32;
    pc_page6[data_offset6 + 4..data_offset6 + 8].copy_from_slice(&str_len6.to_le_bytes());
    pc_page6[str_offset6..str_offset6 + utf16_bytes6.len()].copy_from_slice(&utf16_bytes6);

    // ═══ PAGE 7: Property context for a synthetic message (offset 3584) ═══
    let pc_page7 = &mut pst[3584..4096];
    let bth_offset7: usize = 40;
    let data_offset7 = bth_offset7 + 8;
    let hid_root7: u32 = data_offset7 as u32;
    pc_page7[bth_offset7] = 0xB5;
    pc_page7[bth_offset7 + 1] = 2u8;
    pc_page7[bth_offset7 + 2] = 12u8;
    pc_page7[bth_offset7 + 3] = 0u8;
    pc_page7[bth_offset7 + 4..bth_offset7 + 8].copy_from_slice(&hid_root7.to_le_bytes());

    let props: &[(u32, &str)] = &[
        (
            (PROP_TAG_SUBJECT as u32) << 16 | prop_type::PtypString as u32,
            "Test Subject",
        ),
        (
            (PROP_TAG_MESSAGE_CLASS as u32) << 16 | prop_type::PtypString as u32,
            "IPM.Note",
        ),
        (
            (PROP_TAG_SENDER_NAME as u32) << 16 | prop_type::PtypString as u32,
            "Test Sender",
        ),
        (
            (PROP_TAG_SENDER_EMAIL as u32) << 16 | prop_type::PtypString as u32,
            "sender@test.com",
        ),
    ];

    let mut entry_pos = data_offset7;
    let mut string_storage = entry_pos + props.len() * 12;

    for (tag, value) in props {
        pc_page7[entry_pos..entry_pos + 4].copy_from_slice(&tag.to_le_bytes());
        let utf16: Vec<u8> = format!("{}\0", value)
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let slen = utf16.len() as u32;
        pc_page7[entry_pos + 4..entry_pos + 8].copy_from_slice(&slen.to_le_bytes());
        if string_storage + utf16.len() < 512 {
            pc_page7[string_storage..string_storage + utf16.len()].copy_from_slice(&utf16);
        }
        string_storage += utf16.len();
        entry_pos += 12;
    }

    pst
}

fn bid_to_bytes(bid: u64, buf: &mut [u8]) -> usize {
    let b = bid.to_le_bytes();
    let len = b.len().min(buf.len());
    buf[..len].copy_from_slice(&b[..len]);
    len
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_pst_header_magic() {
        let pst = build_synthetic_unicode_pst();
        assert_eq!(&pst[0..4], &PST_MAGIC);
        let ver = read_u16_le(&pst, 10).unwrap();
        assert_eq!(ver, 23);
    }

    #[test]
    fn open_synthetic_pst() {
        let pst = build_synthetic_unicode_pst();

        // Write to temp file, then open.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();
        assert!(reader.is_unicode());
        assert!(reader.file_size() > 0);
    }

    #[test]
    fn synthetic_pst_nbt_entries() {
        let pst = build_synthetic_unicode_pst();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();

        // The message store NID should be in the NBT cache.
        assert!(reader.nbt_cache.contains_key(&NID_MESSAGE_STORE));
        assert!(reader.nbt_cache.contains_key(&NID_ROOT_FOLDER));
    }

    #[test]
    fn fold_and_read_nbt_structure() {
        let pst = build_synthetic_unicode_pst();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();

        // Verify the NBT root bid from the header.
        let nbt_bid = reader.header.root_nbt.bid;
        assert_eq!(nbt_bid, 4);

        // Verify the BBT root bid.
        let bbt_bid = reader.header.root_bbt.bid;
        assert_eq!(bbt_bid, 2);
    }

    #[test]
    fn synthetic_pst_property_context() {
        let pst = build_synthetic_unicode_pst();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();

        // Read the property context for the message store.
        let block = reader.read_subnode_block(NID_MESSAGE_STORE);
        assert!(block.is_some());

        let props = reader.parse_property_context(block.unwrap());
        assert!(!props.is_empty(), "Property context should have entries");
    }

    #[test]
    fn invalid_pst_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.pst");
        std::fs::write(&path, b"not a pst file").unwrap();

        let result = PstReader::open(&path);
        assert!(result.is_err());
    }

    #[test]
    fn empty_file_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.pst");
        std::fs::write(&path, []).unwrap();

        let result = PstReader::open(&path);
        assert!(result.is_err());
    }
}
