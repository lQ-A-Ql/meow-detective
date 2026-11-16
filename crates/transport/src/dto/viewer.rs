use serde::{Deserialize, Serialize};

pub const MAX_VIEWER_RANGE_LENGTH: u32 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewerPreviewKindDto {
    Image,
    Video,
    Audio,
    Document,
    /// Container metadata is available, but the format has no inline preview.
    ImageMetadataOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerHandleDto {
    pub handle_id: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_kind: Option<ViewerPreviewKindDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerRangeRequestDto {
    pub handle_id: String,
    pub offset: u64,
    pub length: u32,
}

impl ViewerRangeRequestDto {
    /// Validate and normalize a range request before it reaches evidence readers.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.handle_id.trim().is_empty() {
            return Err("handleId is required".to_string());
        }
        if self.length == 0 {
            return Err("length must be greater than zero".to_string());
        }
        self.length = self.length.min(MAX_VIEWER_RANGE_LENGTH);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerRangeResponseDto {
    /// Start offset of the returned bytes.
    #[serde(default)]
    pub offset: u64,
    /// Number of bytes actually returned. This may be shorter than requested.
    #[serde(default)]
    pub bytes_read: u32,
    /// True when the response reaches the catalogued end of the file.
    #[serde(default)]
    pub eof: bool,
    pub kind: String,
    /// Compatibility field for older hex viewers. Bytes-only responses leave this empty.
    pub lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// Raw bytes for hex preview (single-response payload, currently up to 1MB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_bytes: Option<Vec<u8>>,
}

/// Text preview DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPreviewDto {
    /// Text content
    pub content: String,
    /// Encoding name (UTF-8, GBK, etc.)
    pub encoding: String,
    /// Whether content was truncated
    pub is_truncated: bool,
    /// Number of lines
    pub line_count: usize,
    /// Whether file is binary
    pub is_binary: bool,
    /// Programming language (for syntax highlighting)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Hex dump of first 64KB for binary files
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hex_dump: Option<String>,
}

/// Structured table payload for sheet/database-table sections, letting the
/// frontend render real tables instead of joined text lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTableDto {
    /// Column headers (spreadsheet letters or database column names)
    pub columns: Vec<String>,
    /// Bounded cell-grid rows; each row aligns with `columns`
    pub rows: Vec<Vec<String>>,
}

/// A titled text section inside a document preview (page, sheet, table, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSectionDto {
    /// Section title (e.g. "Page 1", "Sheet: Summary", "Table: logins")
    pub title: String,
    /// Bounded text lines of the section
    pub lines: Vec<String>,
    /// Structured cell grid for spreadsheet/database sections
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<DocumentTableDto>,
}

/// Structured preview for document-like files (PDF, Office Open XML, SQLite).
///
/// This is a bounded text extraction, not a layout renderer; binary payloads
/// and images are never inlined.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPreviewDto {
    /// Document kind: pdf | docx | xlsx | pptx | sqlite
    pub kind: String,
    /// Short summary (e.g. "12 pages", "3 sheets, 2 tables read")
    pub summary: String,
    /// Bounded sections
    pub sections: Vec<DocumentSectionDto>,
    /// Whether content was truncated by the preview bounds
    pub truncated: bool,
    /// Non-fatal per-part warnings
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Image preview DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreviewDto {
    /// Data URL (data:mime;base64,...)
    pub data_url: String,
    /// MIME type
    pub mime_type: String,
    /// Image width (0 if unknown)
    pub width: u32,
    /// Image height (0 if unknown)
    pub height: u32,
    /// File size in bytes
    pub size: u64,
}

/// Bounded image metadata extracted from the evidence bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMetadataDto {
    pub status: ImageMetadataStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_time_original: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modify_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gps_date_time: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ImageMetadataWarningDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exif_blocks: Vec<ImageExifBlockDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structure: Option<ImageStructureDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageMetadataStatusDto {
    Present,
    Absent,
    Unsupported,
    Corrupt,
    Oversized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageIssueSeverityDto {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMetadataWarningDto {
    pub code: String,
    pub severity: ImageIssueSeverityDto,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageExifBlockDto {
    pub source: String,
    pub offset: u64,
    pub length: u64,
    pub decoded: bool,
    pub field_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ImageMetadataWarningDto>,
}

/// Bounded container-level (S0) structure evidence for one image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStructureDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_end: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_bytes: Option<u64>,
    pub truncated: bool,
    pub segment_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<ImageSegmentDto>,
    pub segments_truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<ImageStructureFindingDto>,
    pub hashes: ImageStructureHashesDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSegmentDto {
    pub kind: String,
    pub offset: u64,
    pub length: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStructureFindingDto {
    pub code: String,
    pub severity: ImageIssueSeverityDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStructureHashesDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whole_file_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_hashed_bytes: Option<u64>,
}

/// Media URL DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaUrlDto {
    /// Preview delivery mode.
    pub mode: MediaPreviewModeDto,
    /// Media URL. Present for bounded inline previews and scoped protocol previews.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Opaque viewer handle for command-scoped range reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle_id: Option<String>,
    /// MIME type
    pub mime_type: String,
    /// File size in bytes
    pub size: u64,
    /// Whether the frontend can request bounded byte ranges for this media.
    pub can_read_ranges: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaPreviewModeDto {
    Inline,
    Protocol,
    RangeFallback,
}

/// Raw media byte range request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRangeRequestDto {
    pub handle_id: String,
    pub offset: u64,
    pub length: u32,
}

impl MediaRangeRequestDto {
    /// Validate and normalize a media byte range request.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.handle_id.trim().is_empty() {
            return Err("handleId is required".to_string());
        }
        if self.length == 0 {
            return Err("length must be greater than zero".to_string());
        }
        self.length = self.length.min(MAX_VIEWER_RANGE_LENGTH);
        Ok(())
    }
}

/// Raw media byte range response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRangeResponseDto {
    pub offset: u64,
    pub bytes_base64: String,
    pub bytes_read: u32,
    pub eof: bool,
}

pub const DEFAULT_HEX_SEARCH_MAX_MATCHES: u32 = 1_000;
pub const MAX_HEX_SEARCH_MATCHES: u32 = 5_000;
pub const MAX_HEX_SEARCH_PATTERN_BYTES: usize = 64;
pub const MAX_ASCII_SEARCH_PATTERN_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HexSearchModeDto {
    Hex,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HexSearchPhaseDto {
    Queued,
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

impl HexSearchPhaseDto {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HexSearchRequestDto {
    pub file_id: String,
    pub mode: HexSearchModeDto,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_matches: Option<u32>,
}

impl HexSearchRequestDto {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_id.trim().is_empty() {
            return Err("fileId is required".to_string());
        }
        self.normalized_pattern().map(|_| ())
    }

    /// Decode the requested pattern into the bytes the scanner matches.
    pub fn normalized_pattern(&self) -> Result<Vec<u8>, String> {
        match self.mode {
            HexSearchModeDto::Hex => normalize_hex_pattern(&self.pattern),
            HexSearchModeDto::Ascii => {
                let bytes = self.pattern.as_bytes().to_vec();
                if bytes.is_empty() {
                    return Err("ASCII pattern is empty".to_string());
                }
                if bytes.len() > MAX_ASCII_SEARCH_PATTERN_BYTES {
                    return Err(format!(
                        "ASCII pattern exceeds {MAX_ASCII_SEARCH_PATTERN_BYTES} bytes"
                    ));
                }
                Ok(bytes)
            }
        }
    }

    #[must_use]
    pub fn max_matches(&self) -> usize {
        self.max_matches
            .unwrap_or(DEFAULT_HEX_SEARCH_MAX_MATCHES)
            .clamp(1, MAX_HEX_SEARCH_MATCHES) as usize
    }
}

fn normalize_hex_pattern(pattern: &str) -> Result<Vec<u8>, String> {
    let mut digits = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(current) = chars.next() {
        if current.is_ascii_whitespace() || current == ',' {
            continue;
        }
        if current == '0' && matches!(chars.peek(), Some('x' | 'X')) {
            chars.next();
            continue;
        }
        if !current.is_ascii_hexdigit() {
            return Err("Hex pattern contains non-hex characters".to_string());
        }
        digits.push(current);
    }
    if digits.is_empty() {
        return Err("Hex pattern is empty".to_string());
    }
    if !digits.len().is_multiple_of(2) {
        return Err("Hex pattern must contain whole bytes".to_string());
    }
    let byte_len = digits.len() / 2;
    if byte_len > MAX_HEX_SEARCH_PATTERN_BYTES {
        return Err(format!(
            "Hex pattern exceeds {MAX_HEX_SEARCH_PATTERN_BYTES} bytes"
        ));
    }
    let raw = digits.as_bytes();
    let mut bytes = Vec::with_capacity(byte_len);
    for pair in raw.chunks(2) {
        bytes.push((hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]));
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => byte - b'A' + 10,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HexSearchFileRequestDto {
    pub file_id: String,
}

impl HexSearchFileRequestDto {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_id.trim().is_empty() {
            return Err("fileId is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HexSearchStatusDto {
    pub task_id: String,
    pub file_id: String,
    pub phase: HexSearchPhaseDto,
    pub scanned_bytes: u64,
    pub total_bytes: u64,
    pub matches: Vec<u64>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/viewer.rs"]
mod tests;
