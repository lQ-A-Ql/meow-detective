/// LVM2 Metadata Area parser.
///
/// The metadata area is a circular buffer containing ASCII text in LVM's
/// custom key-value format. Each metadata area has a committed raw_locn slot 0;
/// later slots are transient/precommit locations and are not used for ordinary
/// discovery. Across metadata areas and PV copies, the highest valid seqno wins.
///
/// MDA Header (512 bytes, located at mda_region.offset):
/// ```text
/// Offset 0-3:    CRC-32 of bytes 4..512 (u32 LE)
/// Offset 4-19:   " LVM2 x[5A%r0N*>" magic (16 bytes)
/// Offset 20-23:  version (u32 LE, typically 1)
/// Offset 24-31:  metadata_area_offset (u64 LE)
/// Offset 32-39:  metadata_area_size (u64 LE)
/// Offset 40-135: 4 × raw_location_descriptors (24 bytes each)
/// Offset 136-511: reserved (zero-filled)
/// ```
use std::io::{Read, Seek, SeekFrom};

use crate::crc;
use crate::error::{LvmError, Result};

// --- Magic ---
const MDA_MAGIC: [u8; 16] = *b" LVM2 x[5A%r0N*>";
const RAW_LOCN_IGNORED: u32 = 0x0000_0001;
const MDA_HEADER_SIZE: u64 = 512;
const MAX_METADATA_TEXT_SIZE: u64 = 16 * 1024 * 1024;

// --- Public types ---

/// Parsed volume group metadata.
#[derive(Debug, Clone)]
pub struct VolumeGroup {
    pub name: String,
    pub id: String,
    /// Extent size in sectors (typically 512-byte sectors).
    pub extent_size: u64,
    /// Monotonic sequence number — highest wins.
    pub seqno: u64,
    pub physical_volumes: Vec<PvMeta>,
    pub logical_volumes: Vec<LvMeta>,
}

#[derive(Debug, Clone)]
pub struct PvMeta {
    pub name: String, // "pv0", "pv1", ...
    pub uuid: String,
    pub pe_start: u64, // sectors
    pub pe_count: u64,
}

#[derive(Debug, Clone)]
pub struct LvMeta {
    pub name: String,
    pub uuid: String,
    pub status: Vec<String>,
    pub role: LvRole,
    pub segments: Vec<SegmentMeta>,
    /// Total size in bytes, derived from segments.
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LvRole {
    /// User-facing logical volume whose extents can be mapped directly.
    Public,
    /// Thin user volume. It is user-facing but requires thin-pool metadata mapping.
    ThinVolume,
    ThinPool,
    ThinData,
    ThinMetadata,
    CacheVolume,
    CachePool,
    CacheData,
    CacheMetadata,
    RaidImage,
    RaidMetadata,
    MirrorImage,
    MirrorLog,
    Snapshot,
    Internal,
}

impl LvRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            LvRole::Public => "public",
            LvRole::ThinVolume => "thin",
            LvRole::ThinPool => "thin-pool",
            LvRole::ThinData => "thin-data",
            LvRole::ThinMetadata => "thin-metadata",
            LvRole::CacheVolume => "cache",
            LvRole::CachePool => "cache-pool",
            LvRole::CacheData => "cache-data",
            LvRole::CacheMetadata => "cache-metadata",
            LvRole::RaidImage => "raid-image",
            LvRole::RaidMetadata => "raid-metadata",
            LvRole::MirrorImage => "mirror-image",
            LvRole::MirrorLog => "mirror-log",
            LvRole::Snapshot => "snapshot",
            LvRole::Internal => "internal",
        }
    }

    pub fn is_internal(&self) -> bool {
        !matches!(
            self,
            LvRole::Public | LvRole::ThinVolume | LvRole::CacheVolume | LvRole::Snapshot
        )
    }
}

impl LvMeta {
    pub fn is_visible(&self) -> bool {
        if self.role.is_internal() {
            return false;
        }
        self.status.is_empty() || self.status.iter().any(|status| status == "VISIBLE")
    }

    pub fn is_directly_mappable(&self) -> bool {
        self.is_visible()
            && matches!(self.role, LvRole::Public)
            && self.segments.iter().all(|segment| {
                matches!(
                    segment.seg_type,
                    SegmentType::Linear | SegmentType::Striped { .. }
                )
            })
    }
}

#[derive(Debug, Clone)]
pub struct SegmentMeta {
    pub start_extent: u64,
    pub extent_count: u64,
    pub seg_type: SegmentType,
    /// List of (pv_name, start_extent) pairs.
    pub stripes: Vec<(String, u64)>,
}

#[derive(Debug, Clone)]
pub enum SegmentType {
    Linear,
    Striped {
        stripe_count: u64,
        stripe_size: u64,
    },
    /// RAID 0 — stripe_count > 1, no redundancy
    Raid0 {
        stripe_count: u64,
        stripe_size: u64,
    },
    /// RAID 1 — mirroring
    Raid1 {
        mirror_count: u64,
    },
    /// RAID 5 — distributed parity, single-disk fault tolerance
    Raid5 {
        stripe_count: u64,
    },
    /// RAID 6 — double distributed parity
    Raid6 {
        stripe_count: u64,
    },
    /// RAID 10 — striped mirrors
    Raid10 {
        stripe_count: u64,
        mirror_count: u64,
    },
    /// Thin-provisioned user volume (requires thin pool metadata).
    ThinVolume,
    /// Thin pool backing device.
    ThinPool,
    /// Snapshot (CoW origin)
    Snapshot,
    /// Cache user volume.
    CacheVolume,
    /// Cache pool (dm-cache).
    CachePool,
    /// Unknown or unsupported segment type
    Unsupported {
        type_name: String,
    },
}

// --- Public API ---

/// Parse metadata from a metadata area region.
///
/// Reads the MDA header, reads the committed raw_locn slot 0, and returns the
/// parsed volume group.
///
/// `pv_offset` is the byte offset of the PV start in the reader (needed
/// because all LVM2 offsets are absolute from the PV start).
pub fn parse_metadata<R: Read + Seek>(
    reader: &mut R,
    mda_region: &super::label::DataRegion,
    pv_offset: u64,
) -> Result<VolumeGroup> {
    match parse_metadata_region(reader, mda_region, pv_offset) {
        Ok(Some(vg)) => Ok(vg),
        Ok(None) => Err(LvmError::MetadataParseError {
            line: 0,
            message: "no valid committed metadata copy found".to_string(),
        }),
        Err(err) => Err(err.into_lvm_error()),
    }
}

pub(crate) fn parse_metadata_from_regions<R: Read + Seek>(
    reader: &mut R,
    mda_regions: &[super::label::DataRegion],
    pv_offset: u64,
) -> Result<VolumeGroup> {
    let mut best_vg: Option<VolumeGroup> = None;

    for mda_region in mda_regions {
        match parse_metadata_region(reader, mda_region, pv_offset) {
            Ok(Some(vg)) => {
                if best_vg
                    .as_ref()
                    .is_none_or(|current| vg.seqno > current.seqno)
                {
                    best_vg = Some(vg);
                }
            }
            Ok(None) | Err(MetadataRegionError::Recoverable(_)) => {
                // Redundant MDA/PV copies are expected; one corrupt or
                // unreadable copy must not abort discovery when another works.
                continue;
            }
            Err(MetadataRegionError::Fatal(err)) => return Err(err),
        };
    }

    best_vg.ok_or_else(|| LvmError::MetadataParseError {
        line: 0,
        message: "no valid metadata copy found".to_string(),
    })
}

fn parse_metadata_region<R: Read + Seek>(
    reader: &mut R,
    mda_region: &super::label::DataRegion,
    pv_offset: u64,
) -> std::result::Result<Option<VolumeGroup>, MetadataRegionError> {
    // Read MDA header at PV-absolute offset
    let abs_offset = pv_offset.checked_add(mda_region.offset).ok_or_else(|| {
        recoverable_metadata_error("metadata area offset overflows reader address".to_string())
    })?;
    reader
        .seek(SeekFrom::Start(abs_offset))
        .map_err(|err| MetadataRegionError::Recoverable(LvmError::Io(err)))?;
    let mut header = [0u8; 512];
    reader
        .read_exact(&mut header)
        .map_err(|err| MetadataRegionError::Recoverable(LvmError::Io(err)))?;

    // Validate MDA magic
    if header[4..20] != MDA_MAGIC {
        return Err(recoverable_metadata_error(
            "MDA header magic mismatch".to_string(),
        ));
    }

    // Verify MDA CRC
    if !crc::verify_mda_header_crc(&header) {
        let stored = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let computed = crc::lvm_crc32(&header[4..512]);
        return Err(MetadataRegionError::Recoverable(LvmError::MdaCrcMismatch {
            expected: stored,
            actual: computed,
        }));
    }

    let mda_base = u64::from_le_bytes([
        header[24], header[25], header[26], header[27], header[28], header[29], header[30],
        header[31],
    ]);
    let mda_size = u64::from_le_bytes([
        header[32], header[33], header[34], header[35], header[36], header[37], header[38],
        header[39],
    ]);
    if mda_size <= MDA_HEADER_SIZE {
        return Err(recoverable_metadata_error(format!(
            "metadata area size {} is too small",
            mda_size
        )));
    }
    if mda_base != mda_region.offset {
        return Err(recoverable_metadata_error(format!(
            "metadata area header base {} does not match label offset {}",
            mda_base, mda_region.offset
        )));
    }
    if mda_region.size != 0 && mda_size > mda_region.size {
        return Err(recoverable_metadata_error(format!(
            "metadata area header size {} exceeds label region size {}",
            mda_size, mda_region.size
        )));
    }

    // Ordinary discovery uses the committed copy in raw_locn slot 0 only.
    // Slot 1 may contain a higher-seqno precommit image and must not win.
    let locn = RawLocation::from_bytes(&header, 40);
    if locn.is_ignored()
        || locn.is_empty()
        || !locn.is_within(mda_size)
        || locn.size > MAX_METADATA_TEXT_SIZE
    {
        return Ok(None);
    }

    let text_bytes = read_raw_location(reader, pv_offset, mda_base, mda_size, &locn)
        .map_err(MetadataRegionError::Recoverable)?;

    let computed_crc = crc::lvm_crc32(&text_bytes);
    if computed_crc != locn.checksum {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&text_bytes);
    parse_metadata_text(&text)
        .map(Some)
        .map_err(fatal_metadata_region_error)
}

enum MetadataRegionError {
    Recoverable(LvmError),
    Fatal(LvmError),
}

impl MetadataRegionError {
    fn into_lvm_error(self) -> LvmError {
        match self {
            MetadataRegionError::Recoverable(err) | MetadataRegionError::Fatal(err) => err,
        }
    }
}

fn recoverable_metadata_error(message: String) -> MetadataRegionError {
    MetadataRegionError::Recoverable(LvmError::MetadataParseError { line: 0, message })
}

fn fatal_metadata_region_error(err: LvmError) -> MetadataRegionError {
    match err {
        LvmError::MetadataParseError { line, message } => {
            MetadataRegionError::Fatal(LvmError::FatalMetadataParseError { line, message })
        }
        other => MetadataRegionError::Fatal(other),
    }
}

// --- Raw location descriptor ---

struct RawLocation {
    offset: u64,
    size: u64,
    checksum: u32,
    flags: u32,
}

impl RawLocation {
    fn from_bytes(header: &[u8], offset: usize) -> Self {
        Self {
            offset: u64::from_le_bytes([
                header[offset],
                header[offset + 1],
                header[offset + 2],
                header[offset + 3],
                header[offset + 4],
                header[offset + 5],
                header[offset + 6],
                header[offset + 7],
            ]),
            size: u64::from_le_bytes([
                header[offset + 8],
                header[offset + 9],
                header[offset + 10],
                header[offset + 11],
                header[offset + 12],
                header[offset + 13],
                header[offset + 14],
                header[offset + 15],
            ]),
            checksum: u32::from_le_bytes([
                header[offset + 16],
                header[offset + 17],
                header[offset + 18],
                header[offset + 19],
            ]),
            flags: u32::from_le_bytes([
                header[offset + 20],
                header[offset + 21],
                header[offset + 22],
                header[offset + 23],
            ]),
        }
    }

    fn is_ignored(&self) -> bool {
        self.flags & RAW_LOCN_IGNORED != 0
    }

    fn is_empty(&self) -> bool {
        self.offset == 0 && self.size == 0 && self.checksum == 0
    }

    fn is_within(&self, mda_size: u64) -> bool {
        if self.size == 0 || self.offset < MDA_HEADER_SIZE || self.offset >= mda_size {
            return false;
        }
        self.size <= mda_size - MDA_HEADER_SIZE
    }
}

fn read_raw_location<R: Read + Seek>(
    reader: &mut R,
    pv_offset: u64,
    mda_base: u64,
    mda_size: u64,
    locn: &RawLocation,
) -> Result<Vec<u8>> {
    let mut remaining = locn.size;
    let mut raw_offset = locn.offset;
    let mut text_bytes = Vec::with_capacity(locn.size as usize);

    while remaining > 0 {
        let available =
            mda_size
                .checked_sub(raw_offset)
                .ok_or_else(|| LvmError::MetadataParseError {
                    line: 0,
                    message: format!(
                        "raw metadata location offset {} exceeds metadata area size {}",
                        raw_offset, mda_size
                    ),
                })?;
        let chunk_len = remaining.min(available);
        let abs_offset = pv_offset
            .checked_add(mda_base)
            .and_then(|offset| offset.checked_add(raw_offset))
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "raw metadata location overflows reader address".to_string(),
            })?;
        reader.seek(SeekFrom::Start(abs_offset))?;
        let current_len = text_bytes.len();
        text_bytes.resize(current_len + chunk_len as usize, 0);
        reader.read_exact(&mut text_bytes[current_len..])?;

        remaining -= chunk_len;
        raw_offset += chunk_len;
        if remaining > 0 {
            raw_offset = MDA_HEADER_SIZE;
        }
    }

    Ok(text_bytes)
}

// --- ASCII Metadata Text Parser ---

/// Simple recursive-descent parser for LVM2 metadata text format.
///
/// Grammar (simplified):
/// ```text
/// config  := { param | section }*
/// param   := IDENT '=' value
/// value   := INTEGER | STRING | '[' { value }* ']'
/// section := IDENT '{' config '}'
/// ```
struct Parser<'a> {
    text: &'a str,
    pos: usize,
    line: usize,
}

/// Intermediate parse result for a section block.
struct ParsedSection {
    name: String,
    params: Vec<(String, String)>,
    pv_sections: Vec<(String, Vec<(String, String)>)>,
    lv_sections: Vec<LvSectionRaw>,
}

struct LvSectionRaw {
    name: String,
    params: Vec<(String, String)>,
    segments: Vec<SegmentRaw>,
}

#[derive(Debug)]
struct SegmentRaw {
    name: String,
    params: Vec<(String, String)>,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            pos: 0,
            line: 1,
        }
    }

    fn error(&self, msg: &str) -> LvmError {
        LvmError::MetadataParseError {
            line: self.line,
            message: msg.to_string(),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len() {
            match bytes[self.pos] {
                b' ' | b'\t' | b'\r' => {
                    self.pos += 1;
                }
                b'\n' => {
                    self.pos += 1;
                    self.line += 1;
                }
                b'#' => {
                    while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn expect_ident(&mut self) -> std::result::Result<String, LvmError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len()
            && (bytes[self.pos].is_ascii_alphanumeric()
                || bytes[self.pos] == b'_'
                || bytes[self.pos] == b'-'
                || bytes[self.pos] == b'.'
                || bytes[self.pos] == b'+')
        {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.error("expected identifier"));
        }
        Ok(self.text[start..self.pos].to_string())
    }

    fn expect_char(&mut self, ch: u8) -> std::result::Result<(), LvmError> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.text.len() || self.text.as_bytes()[self.pos] != ch {
            return Err(self.error(&format!("expected '{}'", ch as char)));
        }
        self.pos += 1;
        Ok(())
    }

    fn parse_value(&mut self) -> std::result::Result<String, LvmError> {
        self.skip_whitespace_and_comments();
        let bytes = self.text.as_bytes();
        if self.pos >= bytes.len() {
            return Err(self.error("unexpected end of input"));
        }

        match bytes[self.pos] {
            b'"' => {
                self.pos += 1;
                let start = self.pos;
                while self.pos < bytes.len() && bytes[self.pos] != b'"' {
                    if bytes[self.pos] == b'\n' {
                        self.line += 1;
                    }
                    self.pos += 1;
                }
                let s = self.text[start..self.pos].to_string();
                if self.pos < bytes.len() {
                    self.pos += 1;
                }
                Ok(s)
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_whitespace_and_comments();
                    if self.pos >= bytes.len() {
                        break;
                    }
                    if bytes[self.pos] == b']' {
                        self.pos += 1;
                        break;
                    }
                    if bytes[self.pos] == b',' {
                        self.pos += 1;
                        continue;
                    }
                    items.push(self.parse_value()?);
                }
                Ok(format!("[{}]", items.join(", ")))
            }
            _ if bytes[self.pos].is_ascii_digit() || bytes[self.pos] == b'-' => {
                let start = self.pos;
                if bytes[self.pos] == b'-' {
                    self.pos += 1;
                }
                while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
                Ok(self.text[start..self.pos].to_string())
            }
            _ => Err(self.error(&format!(
                "unexpected character '{}'",
                bytes[self.pos] as char
            ))),
        }
    }

    fn parse_param(&mut self) -> std::result::Result<(String, String), LvmError> {
        let key = self.expect_ident()?;
        self.expect_char(b'=')?;
        let value = self.parse_value()?;
        Ok((key, value))
    }

    fn parse_section(&mut self) -> std::result::Result<ParsedSection, LvmError> {
        let name = self.expect_ident()?;
        self.expect_char(b'{')?;

        let mut params = Vec::new();
        let mut pv_sections = Vec::new();
        let mut lv_sections = Vec::new();

        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.text.len() {
                break;
            }
            if self.text.as_bytes()[self.pos] == b'}' {
                self.pos += 1;
                break;
            }

            let peek_start = self.pos;
            let ident = self.expect_ident()?;
            self.skip_whitespace_and_comments();

            if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'=' {
                self.pos = peek_start;
                let (k, v) = self.parse_param()?;
                params.push((k, v));
            } else if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'{' {
                // Don't rewind — we're already past the subsection name, at '{'
                if ident == "physical_volumes" {
                    self.expect_char(b'{')?;
                    loop {
                        self.skip_whitespace_and_comments();
                        if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'}' {
                            self.pos += 1;
                            break;
                        }
                        let pv_ident = self.expect_ident()?;
                        self.expect_char(b'{')?;
                        let mut pv_params = Vec::new();
                        loop {
                            self.skip_whitespace_and_comments();
                            if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'}'
                            {
                                self.pos += 1;
                                break;
                            }
                            let (k, v) = self.parse_param()?;
                            pv_params.push((k, v));
                        }
                        pv_sections.push((pv_ident, pv_params));
                    }
                } else if ident == "logical_volumes" {
                    self.expect_char(b'{')?;
                    loop {
                        self.skip_whitespace_and_comments();
                        if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'}' {
                            self.pos += 1;
                            break;
                        }
                        let lv_name = self.expect_ident()?;
                        self.expect_char(b'{')?;
                        let mut lv_params = Vec::new();
                        let mut segments = Vec::new();
                        loop {
                            self.skip_whitespace_and_comments();
                            if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'}'
                            {
                                self.pos += 1;
                                break;
                            }
                            let peek_start = self.pos;
                            let key = self.expect_ident()?;
                            self.skip_whitespace_and_comments();
                            if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'='
                            {
                                self.pos = peek_start;
                                let (k, v) = self.parse_param()?;
                                lv_params.push((k, v));
                            } else if key.starts_with("segment") {
                                self.expect_char(b'{')?;
                                let mut seg_params = Vec::new();
                                loop {
                                    self.skip_whitespace_and_comments();
                                    if self.pos < self.text.len()
                                        && self.text.as_bytes()[self.pos] == b'}'
                                    {
                                        self.pos += 1;
                                        break;
                                    }
                                    let (k, v) = self.parse_param()?;
                                    seg_params.push((k, v));
                                }
                                segments.push(SegmentRaw {
                                    name: key,
                                    params: seg_params,
                                });
                            } else {
                                break;
                            }
                        }
                        lv_sections.push(LvSectionRaw {
                            name: lv_name,
                            params: lv_params,
                            segments,
                        });
                    }
                } else {
                    // Unknown section — skip contents
                    let mut depth = 1u32;
                    while self.pos < self.text.len() && depth > 0 {
                        match self.text.as_bytes()[self.pos] {
                            b'{' => depth += 1,
                            b'}' => depth -= 1,
                            b'\n' => self.line += 1,
                            _ => {}
                        }
                        self.pos += 1;
                    }
                }
            }
        }

        Ok(ParsedSection {
            name,
            params,
            pv_sections,
            lv_sections,
        })
    }
}

/// Parse LVM2 metadata text into a VolumeGroup.
fn parse_metadata_text(text: &str) -> Result<VolumeGroup> {
    let mut parser: Parser<'_> = Parser::new(text);

    // Find the VG section: skip all top-level params (key=value lines)
    // and locate the first `identifier {` block.
    parser.skip_whitespace_and_comments();
    while parser.pos < text.len() {
        let saved = parser.pos;
        let _ident = parser.expect_ident()?;
        parser.skip_whitespace_and_comments();
        if parser.pos < text.len() && text.as_bytes()[parser.pos] == b'{' {
            // This is the VG section — rewind to start of its name
            parser.pos = saved;
            break;
        } else if parser.pos < text.len() && text.as_bytes()[parser.pos] == b'=' {
            // Top-level param — parse and skip value
            parser.pos = saved;
            let _ = parser.parse_param();
            parser.skip_whitespace_and_comments();
        }
    }

    let section = parser.parse_section()?;

    let id = required_string(&section.params, "id", "volume group")?;
    let seqno = required_u64(&section.params, "seqno", "volume group")?;
    let extent_size = required_u64(&section.params, "extent_size", "volume group")?;

    let physical_volumes: Vec<PvMeta> = section
        .pv_sections
        .iter()
        .map(|(name, params)| {
            Ok(PvMeta {
                uuid: required_string(params, "id", &format!("physical volume '{}'", name))?,
                pe_start: required_u64(params, "pe_start", &format!("physical volume '{}'", name))?,
                pe_count: required_u64(params, "pe_count", &format!("physical volume '{}'", name))?,
                name: name.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let extent_size_bytes =
        extent_size
            .checked_mul(512u64)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "volume group extent size overflows bytes".to_string(),
            })?;
    let logical_volumes: Vec<LvMeta> = section
        .lv_sections
        .iter()
        .map(|lv_raw| parse_logical_volume(lv_raw, extent_size_bytes))
        .collect::<Result<Vec<_>>>()?;

    Ok(VolumeGroup {
        name: section.name,
        id,
        extent_size,
        seqno,
        physical_volumes,
        logical_volumes,
    })
}

fn parse_logical_volume(lv_raw: &LvSectionRaw, extent_size_bytes: u64) -> Result<LvMeta> {
    let context = format!("logical volume '{}'", lv_raw.name);
    let lv_uuid = required_string(&lv_raw.params, "id", &context)?;
    let status = optional_list(&lv_raw.params, "status");
    let role = infer_lv_role(lv_raw);
    let declared_segment_count = required_u64(&lv_raw.params, "segment_count", &context)?;

    let mut segs = Vec::with_capacity(lv_raw.segments.len());
    let mut unsupported_reason = None;
    for segment in &lv_raw.segments {
        match parse_segment(segment, &context) {
            Ok(segment_meta) => segs.push(segment_meta),
            Err(SegmentParseError::Unsupported { segment, reason }) => {
                segs.push(segment);
                if unsupported_reason.is_none() {
                    unsupported_reason = Some(reason);
                }
            }
            Err(SegmentParseError::Fatal(err)) => return Err(err),
        }
    }

    if declared_segment_count != lv_raw.segments.len() as u64 {
        unsupported_reason = Some(format!(
            "{} declares segment_count {} but contains {} segment blocks",
            context,
            declared_segment_count,
            lv_raw.segments.len()
        ));
    } else if let Some(type_name) = segs.iter().find_map(unsupported_segment_type_name) {
        unsupported_reason = Some(format!(
            "{} uses unsupported segment type '{}'",
            context, type_name
        ));
    } else if let Err(err) = validate_segment_layout(&segs, &context) {
        unsupported_reason = Some(err);
    }

    if let Some(reason) = unsupported_reason {
        let size_extents = max_segment_end(&segs)?;
        segs = vec![unsupported_lv_segment(size_extents, reason)];
    }

    let size_bytes = logical_volume_size_bytes(&segs, extent_size_bytes)?;
    Ok(LvMeta {
        name: lv_raw.name.clone(),
        uuid: lv_uuid,
        status,
        role,
        segments: segs,
        size_bytes,
    })
}

fn optional_list(params: &[(String, String)], key: &str) -> Vec<String> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, value)| parse_metadata_list_value(value))
        .unwrap_or_default()
}

fn parse_metadata_list_value(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_ascii_uppercase())
        .filter(|item| !item.is_empty())
        .collect()
}

fn infer_lv_role(lv_raw: &LvSectionRaw) -> LvRole {
    let name = lv_raw.name.as_str();
    if name.starts_with('[') && name.ends_with(']') {
        return LvRole::Internal;
    }
    if name.ends_with("_tdata") {
        return LvRole::ThinData;
    }
    if name.ends_with("_tmeta") {
        return LvRole::ThinMetadata;
    }
    if name.ends_with("_cdata") {
        return LvRole::CacheData;
    }
    if name.ends_with("_cmeta") {
        return LvRole::CacheMetadata;
    }
    if name.contains("_rimage_") {
        return LvRole::RaidImage;
    }
    if name.contains("_rmeta_") {
        return LvRole::RaidMetadata;
    }
    if name.contains("_mimage_") {
        return LvRole::MirrorImage;
    }
    if name.ends_with("_mlog") {
        return LvRole::MirrorLog;
    }
    for segment in &lv_raw.segments {
        if let Some((_, segment_type)) = segment.params.iter().find(|(key, _)| key == "type") {
            match segment_type.as_str() {
                "thin" => return LvRole::ThinVolume,
                "thin-pool" => return LvRole::ThinPool,
                "cache" => return LvRole::CacheVolume,
                "cache-pool" => return LvRole::CachePool,
                "snapshot" => return LvRole::Snapshot,
                _ => {}
            }
        }
    }

    let status = optional_list(&lv_raw.params, "status");
    if !status.is_empty() && !status.iter().any(|item| item == "VISIBLE") {
        return LvRole::Internal;
    }

    LvRole::Public
}

fn required_string(params: &[(String, String)], key: &str, context: &str) -> Result<String> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("missing required field '{}' in {}", key, context),
        })
}

fn required_u64(params: &[(String, String)], key: &str, context: &str) -> Result<u64> {
    let value = required_string(params, key, context)?;
    value
        .parse::<u64>()
        .map_err(|_| LvmError::MetadataParseError {
            line: 0,
            message: format!("invalid integer field '{}' in {}", key, context),
        })
}

fn optional_u64(params: &[(String, String)], key: &str) -> Option<u64> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, value)| value.parse::<u64>().ok())
}

fn logical_volume_size_bytes(segs: &[SegmentMeta], extent_size_bytes: u64) -> Result<u64> {
    let mut max_end = 0u64;
    for segment in segs {
        let end_extent = segment
            .start_extent
            .checked_add(segment.extent_count)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "logical volume extent range overflows u64".to_string(),
            })?;
        max_end = max_end.max(end_extent);
    }
    max_end
        .checked_mul(extent_size_bytes)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: "logical volume byte size overflows u64".to_string(),
        })
}

enum SegmentParseError {
    Unsupported {
        segment: SegmentMeta,
        reason: String,
    },
    Fatal(LvmError),
}

impl SegmentParseError {
    fn fatal(message: String) -> Self {
        SegmentParseError::Fatal(LvmError::MetadataParseError { line: 0, message })
    }
}

fn parse_segment(
    seg: &SegmentRaw,
    lv_context: &str,
) -> std::result::Result<SegmentMeta, SegmentParseError> {
    let context = format!("{} segment '{}'", lv_context, seg.name);
    let start_extent =
        required_u64(&seg.params, "start_extent", &context).map_err(SegmentParseError::Fatal)?;
    let extent_count =
        required_u64(&seg.params, "extent_count", &context).map_err(SegmentParseError::Fatal)?;
    if extent_count == 0 {
        return Err(SegmentParseError::fatal(format!(
            "extent_count must be greater than zero in {}",
            context
        )));
    }
    let type_name =
        required_string(&seg.params, "type", &context).map_err(SegmentParseError::Fatal)?;

    let (seg_type, stripes) = match type_name.as_str() {
        "linear" => {
            let stripe_count = required_u64(&seg.params, "stripe_count", &context)
                .map_err(SegmentParseError::Fatal)?;
            if stripe_count != 1 {
                return Err(SegmentParseError::fatal(format!(
                    "linear stripe_count must be 1 in {}",
                    context
                )));
            }
            let stripes = parse_required_stripes(&seg.params, &context, stripe_count)
                .map_err(SegmentParseError::Fatal)?;
            (SegmentType::Linear, stripes)
        }
        "striped" => {
            let stripe_count = required_u64(&seg.params, "stripe_count", &context)
                .map_err(SegmentParseError::Fatal)?;
            if stripe_count == 0 {
                return Err(SegmentParseError::fatal(format!(
                    "stripe_count must be greater than zero in {}",
                    context
                )));
            }
            let stripes = parse_required_stripes(&seg.params, &context, stripe_count)
                .map_err(SegmentParseError::Fatal)?;
            if stripe_count == 1 {
                (SegmentType::Linear, stripes)
            } else {
                let stripe_size = match required_stripe_size(&seg.params, &context) {
                    Ok(stripe_size) => stripe_size,
                    Err(err) => {
                        let reason = lvm_error_message(&err);
                        return Err(SegmentParseError::Unsupported {
                            segment: unsupported_segment(
                                start_extent,
                                extent_count,
                                reason.clone(),
                            ),
                            reason,
                        });
                    }
                };
                (
                    SegmentType::Striped {
                        stripe_count,
                        stripe_size,
                    },
                    stripes,
                )
            }
        }
        "raid0" => parse_raid_segment(
            SegmentType::Raid0 {
                stripe_count: required_u64(&seg.params, "stripe_count", &context)
                    .map_err(SegmentParseError::Fatal)?,
                stripe_size: required_stripe_size(&seg.params, &context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            &seg.params,
            &context,
            start_extent,
            extent_count,
        )?,
        "raid1" | "mirror" => unsupported_raid_or_mirror_segment(
            &seg.params,
            &context,
            start_extent,
            extent_count,
            "raid1/mirror requires LVM component LV mapping and sync-state validation",
        )?,
        "raid10" => unsupported_raid_or_mirror_segment(
            &seg.params,
            &context,
            start_extent,
            extent_count,
            "raid10 requires LVM component LV mapping and stripe/mirror reconstruction",
        )?,
        "raid5" => parse_raid_segment(
            SegmentType::Raid5 {
                stripe_count: required_u64(&seg.params, "stripe_count", &context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            &seg.params,
            &context,
            start_extent,
            extent_count,
        )?,
        "raid6" => parse_raid_segment(
            SegmentType::Raid6 {
                stripe_count: required_u64(&seg.params, "stripe_count", &context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            &seg.params,
            &context,
            start_extent,
            extent_count,
        )?,
        "thin" | "thin-pool" | "snapshot" | "cache" | "cache-pool" => {
            let seg_type = match type_name.as_str() {
                "thin" => SegmentType::ThinVolume,
                "thin-pool" => SegmentType::ThinPool,
                "snapshot" => SegmentType::Snapshot,
                "cache" => SegmentType::CacheVolume,
                "cache-pool" => SegmentType::CachePool,
                _ => unreachable!(),
            };
            (seg_type, Vec::new())
        }
        other => (
            SegmentType::Unsupported {
                type_name: other.to_string(),
            },
            Vec::new(),
        ),
    };

    Ok(SegmentMeta {
        start_extent,
        extent_count,
        seg_type,
        stripes,
    })
}

fn parse_raid_segment(
    seg_type: SegmentType,
    params: &[(String, String)],
    context: &str,
    start_extent: u64,
    extent_count: u64,
) -> std::result::Result<(SegmentType, Vec<(String, u64)>), SegmentParseError> {
    let stripe_count = match &seg_type {
        SegmentType::Raid0 { stripe_count, .. }
        | SegmentType::Raid5 { stripe_count }
        | SegmentType::Raid6 { stripe_count }
        | SegmentType::Raid10 { stripe_count, .. } => *stripe_count,
        SegmentType::Raid1 { mirror_count } => *mirror_count,
        _ => {
            return Err(SegmentParseError::Fatal(LvmError::MetadataParseError {
                line: 0,
                message: format!("{} is not a RAID segment", context),
            }));
        }
    };

    match parse_required_stripes(params, context, stripe_count) {
        Ok(stripes) => Ok((seg_type, stripes)),
        Err(err) => {
            let reason = lvm_error_message(&err);
            Err(SegmentParseError::Unsupported {
                segment: unsupported_segment(start_extent, extent_count, reason.clone()),
                reason,
            })
        }
    }
}

fn unsupported_raid_or_mirror_segment(
    params: &[(String, String)],
    context: &str,
    start_extent: u64,
    extent_count: u64,
    reason: &str,
) -> std::result::Result<(SegmentType, Vec<(String, u64)>), SegmentParseError> {
    if let Some(stripe_count) = optional_u64(params, "stripe_count") {
        let _ = parse_required_stripes(params, context, stripe_count);
    }
    Err(SegmentParseError::Unsupported {
        segment: unsupported_segment(start_extent, extent_count, reason.to_string()),
        reason: reason.to_string(),
    })
}

fn unsupported_segment(start_extent: u64, extent_count: u64, reason: String) -> SegmentMeta {
    SegmentMeta {
        start_extent,
        extent_count,
        seg_type: SegmentType::Unsupported { type_name: reason },
        stripes: Vec::new(),
    }
}

fn lvm_error_message(err: &LvmError) -> String {
    match err {
        LvmError::MetadataParseError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

fn parse_required_stripes(
    params: &[(String, String)],
    context: &str,
    stripe_count: u64,
) -> Result<Vec<(String, u64)>> {
    let stripes_raw = required_string(params, "stripes", context)?;
    let stripes = parse_stripes_list(&stripes_raw, context)?;
    if stripes.len() != stripe_count as usize {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "{} expects {} stripe entries but found {}",
                context,
                stripe_count,
                stripes.len()
            ),
        });
    }
    Ok(stripes)
}

fn required_stripe_size(params: &[(String, String)], context: &str) -> Result<u64> {
    let stripe_size = required_u64(params, "stripe_size", context)?;
    if stripe_size == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("stripe_size must be greater than zero in {}", context),
        });
    }
    stripe_size
        .checked_mul(512)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("stripe_size overflows bytes in {}", context),
        })?;
    Ok(stripe_size)
}

fn unsupported_segment_type_name(segment: &SegmentMeta) -> Option<&str> {
    match &segment.seg_type {
        SegmentType::Unsupported { type_name } => Some(type_name.as_str()),
        SegmentType::ThinVolume => Some("thin"),
        SegmentType::ThinPool => Some("thin-pool"),
        SegmentType::Snapshot => Some("snapshot"),
        SegmentType::CacheVolume => Some("cache"),
        SegmentType::CachePool => Some("cache-pool"),
        SegmentType::Raid1 { .. } => Some("raid1"),
        SegmentType::Raid10 { .. } => Some("raid10"),
        SegmentType::Raid5 { .. } => Some("raid5"),
        SegmentType::Raid6 { .. } => Some("raid6"),
        SegmentType::Raid0 { .. } => Some("raid0"),
        SegmentType::Linear | SegmentType::Striped { .. } => None,
    }
}

fn unsupported_lv_segment(extent_count: u64, reason: String) -> SegmentMeta {
    SegmentMeta {
        start_extent: 0,
        extent_count,
        seg_type: SegmentType::Unsupported { type_name: reason },
        stripes: Vec::new(),
    }
}

fn max_segment_end(segs: &[SegmentMeta]) -> Result<u64> {
    let mut max_end = 0u64;
    for segment in segs {
        let end = segment
            .start_extent
            .checked_add(segment.extent_count)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "logical volume extent range overflows u64".to_string(),
            })?;
        max_end = max_end.max(end);
    }
    Ok(max_end)
}

fn validate_segment_layout(segs: &[SegmentMeta], context: &str) -> std::result::Result<(), String> {
    if segs.is_empty() {
        return Err(format!("{} contains no segment blocks", context));
    }

    let mut ranges = Vec::with_capacity(segs.len());
    for segment in segs {
        let end = segment
            .start_extent
            .checked_add(segment.extent_count)
            .ok_or_else(|| format!("{} segment extent range overflows u64", context))?;
        ranges.push((segment.start_extent, end));
    }
    ranges.sort_by_key(|(start, _)| *start);

    let mut expected_start = 0u64;
    for (start, end) in ranges {
        if start != expected_start {
            let relation = if start > expected_start {
                "gap"
            } else {
                "overlap"
            };
            return Err(format!(
                "{} has segment {}: expected start_extent {} but found {}",
                context, relation, expected_start, start
            ));
        }
        expected_start = end;
    }

    Ok(())
}

/// Parse "pv0, 0, pv1, 1024" into [(pv0, 0), (pv1, 1024)].
fn parse_stripes_list(raw: &str, context: &str) -> Result<Vec<(String, u64)>> {
    let clean = raw.trim_matches(|c| c == '[' || c == ']' || c == '"');
    if clean.is_empty() {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("missing stripes entries in {}", context),
        });
    }
    let parts: Vec<&str> = clean
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .collect();
    if !parts.len().is_multiple_of(2) {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("stripes list has an odd number of entries in {}", context),
        });
    }
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < parts.len() {
        let pv = parts[i].to_string();
        if pv.is_empty() {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!("empty PV name in stripes list in {}", context),
            });
        }
        let extent = parts[i + 1]
            .parse::<u64>()
            .map_err(|_| LvmError::MetadataParseError {
                line: 0,
                message: format!("invalid stripe extent in {}", context),
            })?;
        result.push((pv, extent));
        i += 2;
    }
    Ok(result)
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod metadata_tests;
