/// LVM2 Metadata Area parser.
///
/// The metadata area is a circular buffer containing ASCII text in LVM's
/// custom key-value format. Multiple copies may exist (with different seqno
/// values); the one with the highest seqno is authoritative.
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
    pub name: String,       // "pv0", "pv1", ...
    pub uuid: String,
    pub pe_start: u64,      // sectors
    pub pe_count: u64,
}

#[derive(Debug, Clone)]
pub struct LvMeta {
    pub name: String,
    pub uuid: String,
    pub segments: Vec<SegmentMeta>,
    /// Total size in bytes, derived from segments.
    pub size_bytes: u64,
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
    Striped { stripe_count: u64 },
    Unsupported { type_name: String },
}

// --- Public API ---

/// Parse metadata from a metadata area region.
///
/// Reads the MDA header, locates all raw location descriptors, reads their
/// data blocks, and returns the parsed volume group (picking the copy with
/// the highest seqno).
pub fn parse_metadata<R: Read + Seek>(
    reader: &mut R,
    mda_region: &super::label::DataRegion,
) -> Result<VolumeGroup> {
    // Read MDA header
    reader.seek(SeekFrom::Start(mda_region.offset))?;
    let mut header = [0u8; 512];
    reader.read_exact(&mut header)?;

    // Validate MDA magic
    if header[4..20] != MDA_MAGIC {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: "MDA header magic mismatch".to_string(),
        });
    }

    // Verify MDA CRC
    if !crc::verify_mda_header_crc(&header) {
        let stored = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let computed = crc::lvm_crc32(&header[4..512]);
        return Err(LvmError::MdaCrcMismatch {
            expected: stored,
            actual: computed,
        });
    }

    let mda_base = u64::from_le_bytes([
        header[24], header[25], header[26], header[27],
        header[28], header[29], header[30], header[31],
    ]);

    // Parse up to 4 raw location descriptors
    let mut best_vg: Option<VolumeGroup> = None;
    let mut best_seqno: i64 = -1;

    for i in 0..4 {
        let rl_offset = 40 + i * 24;
        let locn = RawLocation::from_bytes(&header, rl_offset);

        // Skip ignored or empty descriptors
        if locn.is_ignored() || locn.is_empty() {
            continue;
        }

        // Read the metadata text
        let abs_offset = mda_base + locn.offset;
        if locn.size == 0 || locn.size > 16 * 1024 * 1024 {
            // safety: max 16 MB metadata
            continue;
        }

        reader.seek(SeekFrom::Start(abs_offset))?;
        let mut text_bytes = vec![0u8; locn.size as usize];
        if reader.read_exact(&mut text_bytes).is_err() {
            continue; // skip unreadable descriptors
        }

        // Verify metadata CRC
        let computed_crc = crc::lvm_crc32(&text_bytes);
        if computed_crc != locn.checksum {
            continue; // skip corrupted copies, try next
        }

        // Parse the ASCII text
        let text = String::from_utf8_lossy(&text_bytes);
        match parse_metadata_text(&text) {
            Ok(vg) => {
                if vg.seqno as i64 > best_seqno {
                    best_seqno = vg.seqno as i64;
                    best_vg = Some(vg);
                }
            }
            Err(_e) => {
                // Try next copy if this one can't be parsed
                continue;
            }
        }
    }

    best_vg.ok_or_else(|| LvmError::MetadataParseError {
        line: 0,
        message: "no valid metadata copy found".to_string(),
    })
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
                header[offset], header[offset + 1], header[offset + 2], header[offset + 3],
                header[offset + 4], header[offset + 5], header[offset + 6], header[offset + 7],
            ]),
            size: u64::from_le_bytes([
                header[offset + 8], header[offset + 9], header[offset + 10], header[offset + 11],
                header[offset + 12], header[offset + 13], header[offset + 14], header[offset + 15],
            ]),
            checksum: u32::from_le_bytes([
                header[offset + 16], header[offset + 17], header[offset + 18], header[offset + 19],
            ]),
            flags: u32::from_le_bytes([
                header[offset + 20], header[offset + 21], header[offset + 22], header[offset + 23],
            ]),
        }
    }

    fn is_ignored(&self) -> bool {
        self.flags & RAW_LOCN_IGNORED != 0
    }

    fn is_empty(&self) -> bool {
        self.offset == 0 && self.size == 0 && self.checksum == 0
    }
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
    #[allow(dead_code)]
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
        Self { text, pos: 0, line: 1 }
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
                b' ' | b'\t' | b'\r' => { self.pos += 1; }
                b'\n' => { self.pos += 1; self.line += 1; }
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
                || bytes[self.pos] == b'.')
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
                    if bytes[self.pos] == b'\n' { self.line += 1; }
                    self.pos += 1;
                }
                let s = self.text[start..self.pos].to_string();
                if self.pos < bytes.len() { self.pos += 1; }
                Ok(s)
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_whitespace_and_comments();
                    if self.pos >= bytes.len() { break; }
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
                if bytes[self.pos] == b'-' { self.pos += 1; }
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
            if self.pos >= self.text.len() { break; }
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
                            if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'}' {
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
                            if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'}' {
                                self.pos += 1;
                                break;
                            }
                            let peek_start = self.pos;
                            let key = self.expect_ident()?;
                            self.skip_whitespace_and_comments();
                            if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'=' {
                                self.pos = peek_start;
                                let (k, v) = self.parse_param()?;
                                lv_params.push((k, v));
                            } else if key.starts_with("segment") {
                                self.expect_char(b'{')?;
                                let mut seg_params = Vec::new();
                                loop {
                                    self.skip_whitespace_and_comments();
                                    if self.pos < self.text.len() && self.text.as_bytes()[self.pos] == b'}' {
                                        self.pos += 1;
                                        break;
                                    }
                                    let (k, v) = self.parse_param()?;
                                    seg_params.push((k, v));
                                }
                                segments.push(SegmentRaw { name: key, params: seg_params });
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

        Ok(ParsedSection { name, params, pv_sections, lv_sections })
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
        let ident = parser.expect_ident()?;
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

    let id = find_param(&section.params, "id");
    let seqno = find_param(&section.params, "seqno")
        .parse::<u64>().unwrap_or(0);
    let extent_size = find_param(&section.params, "extent_size")
        .parse::<u64>().unwrap_or(8192);

    let physical_volumes: Vec<PvMeta> = section.pv_sections
        .iter()
        .map(|(name, params)| PvMeta {
            uuid: find_param(params, "id"),
            pe_start: find_param(params, "pe_start").parse().unwrap_or(0),
            pe_count: find_param(params, "pe_count").parse().unwrap_or(0),
            name: name.clone(),
        })
        .collect();

    let extent_size_bytes = extent_size * 512u64;
    let logical_volumes: Vec<LvMeta> = section.lv_sections
        .iter()
        .map(|lv_raw| {
            let lv_uuid = find_param(&lv_raw.params, "id");
            let segs: Vec<SegmentMeta> = lv_raw.segments
                .iter()
                .map(|s| parse_segment(s, extent_size_bytes))
                .collect();
            let size_bytes: u64 = segs.iter().map(|s| s.extent_count * extent_size_bytes).sum();
            LvMeta {
                name: lv_raw.name.clone(),
                uuid: lv_uuid,
                segments: segs,
                size_bytes,
            }
        })
        .collect();

    Ok(VolumeGroup {
        name: section.name,
        id,
        extent_size,
        seqno,
        physical_volumes,
        logical_volumes,
    })
}

fn find_param(params: &[(String, String)], key: &str) -> String {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn parse_segment(
    seg: &SegmentRaw,
    _extent_size_bytes: u64,
) -> SegmentMeta {
    let start_extent: u64 = find_param(&seg.params, "start_extent").parse().unwrap_or(0);
    let extent_count: u64 = find_param(&seg.params, "extent_count").parse().unwrap_or(0);
    let type_name = find_param(&seg.params, "type");
    let stripe_count: u64 = find_param(&seg.params, "stripe_count").parse().unwrap_or(1);

    let seg_type = match (type_name.as_str(), stripe_count) {
        ("striped", 1) | ("linear", _) => SegmentType::Linear,
        ("striped", n) if n > 1 => SegmentType::Striped { stripe_count: n },
        (other, _) => SegmentType::Unsupported {
            type_name: other.to_string(),
        },
    };

    // Parse stripes list
    let stripes_raw = find_param(&seg.params, "stripes");
    let stripes = parse_stripes_list(&stripes_raw);

    SegmentMeta {
        start_extent,
        extent_count,
        seg_type,
        stripes,
    }
}

/// Parse "pv0, 0, pv1, 1024" → [(pv0, 0), (pv1, 1024)]
fn parse_stripes_list(raw: &str) -> Vec<(String, u64)> {
    let clean = raw.trim_matches(|c| c == '[' || c == ']' || c == '"');
    if clean.is_empty() {
        return Vec::new();
    }
    let parts: Vec<&str> = clean.split(',').map(|s| s.trim().trim_matches('"')).collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < parts.len() {
        let pv = parts[i].to_string();
        let extent: u64 = parts[i + 1].parse().unwrap_or(0);
        result.push((pv, extent));
        i += 2;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::label::DataRegion;

    fn build_minimal_metadata_text() -> String {
        r#"contents = "Text Format Volume Group"
version = 1

test_vg {
    id = "vg-uuid-1234-5678-90ab-cdef"
    seqno = 42
    extent_size = 8192

    physical_volumes {
        pv0 {
            id = "pv-uuid-1234-5678-90ab-cdef"
            device = "/dev/sda1"
            pe_start = 2048
            pe_count = 2559
        }
    }

    logical_volumes {
        root {
            id = "lv-root-uuid-1234-5678"
            segment_count = 1
            segment1 {
                start_extent = 0
                extent_count = 1280
                type = "striped"
                stripe_count = 1
                stripes = ["pv0", 0]
            }
        }
        home {
            id = "lv-home-uuid-1234-5678"
            segment_count = 1
            segment1 {
                start_extent = 0
                extent_count = 512
                type = "striped"
                stripe_count = 1
                stripes = ["pv0", 1280]
            }
        }
    }
}
"#
        .to_string()
    }

    #[test]
    fn parse_minimal_metadata() {
        let text = build_minimal_metadata_text();
        let vg = parse_metadata_text(&text).unwrap();

        assert_eq!(vg.name, "test_vg");
        assert_eq!(vg.extent_size, 8192);
        assert_eq!(vg.seqno, 42);
        assert_eq!(vg.physical_volumes.len(), 1);
        assert_eq!(vg.logical_volumes.len(), 2);

        let root = &vg.logical_volumes[0];
        assert_eq!(root.name, "root");
        assert_eq!(root.segments.len(), 1);
        assert_eq!(root.segments[0].extent_count, 1280);
        assert!(matches!(root.segments[0].seg_type, SegmentType::Linear));

        let home = &vg.logical_volumes[1];
        assert_eq!(home.name, "home");
        assert_eq!(home.segments.len(), 1);
        assert_eq!(home.segments[0].extent_count, 512);
    }

    #[test]
    fn metadata_text_lv_sizes() {
        let text = build_minimal_metadata_text();
        let vg = parse_metadata_text(&text).unwrap();

        let extent_bytes = vg.extent_size * 512;
        let root = &vg.logical_volumes[0];
        assert_eq!(root.size_bytes, 1280 * extent_bytes);
        let home = &vg.logical_volumes[1];
        assert_eq!(home.size_bytes, 512 * extent_bytes);
    }
}
