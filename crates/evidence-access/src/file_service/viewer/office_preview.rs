//! Office Open XML (docx/xlsx/pptx) structured preview: bounded text
//! extraction from zip container parts.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::{Cursor, Read};
use transport::dto::{DocumentPreviewDto, DocumentSectionDto};
use zip::ZipArchive;

use super::document::{bounded_line_with_status, DocumentKind, MAX_LINE_CHARS, MAX_SECTION_LINES};
use crate::file_service::FileServiceError;

mod pptx;
mod xlsx;

pub(crate) use pptx::preview_pptx;
pub(crate) use xlsx::preview_xlsx;

const MAX_OFFICE_PARTS: usize = 20;

const MAX_ZIP_ENTRY_TEXT_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) fn preview_office_part(
    bytes: &[u8],
    kind: DocumentKind,
    extract: impl Fn(&str) -> BoundedTextLines,
) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("Office container parse failed: {error}"))
    })?;
    let part_path = match kind {
        DocumentKind::Docx => "word/document.xml",
        _ => unreachable!("preview_office_part only handles docx"),
    };
    let part = read_zip_text(&mut archive, part_path)?;
    let text = extract(&part.text);
    let truncated = part.truncated || text.truncated;
    let mut warnings = part.warnings(part_path);
    if text.line_count_truncated {
        warnings.push(format!(
            "{part_path}: paragraph preview limited to {MAX_SECTION_LINES} lines"
        ));
    }
    if text.line_width_truncated {
        warnings.push(format!(
            "{part_path}: paragraph text limited to {MAX_LINE_CHARS} characters per line"
        ));
    }
    Ok(DocumentPreviewDto {
        kind: kind.as_str().to_string(),
        summary: format!("{} paragraphs", text.lines.len()),
        sections: vec![DocumentSectionDto {
            title: "Content".to_string(),
            lines: text.lines,
            table: None,
        }],
        truncated,
        warnings,
    })
}

pub(super) struct ZipText {
    pub(super) text: String,
    pub(super) truncated: bool,
}

impl ZipText {
    pub(super) fn warnings(&self, path: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.truncated {
            warnings.push(format!(
                "{path}: XML entry limited to {} MiB",
                MAX_ZIP_ENTRY_TEXT_BYTES / (1024 * 1024)
            ));
        }
        if let Some(warning) = xml_parse_warning(path, &self.text) {
            warnings.push(warning);
        }
        warnings
    }
}

pub(crate) fn read_zip_text(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
) -> Result<ZipText, FileServiceError> {
    let entry = archive
        .by_name(path)
        .map_err(|error| FileServiceError::invalid_input(format!("{path}: {error}")))?;
    let declared_size = entry.size();
    let mut bytes = Vec::with_capacity(entry.size().min(MAX_ZIP_ENTRY_TEXT_BYTES) as usize);
    entry
        .take(MAX_ZIP_ENTRY_TEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(FileServiceError::from)?;
    let truncated =
        declared_size > MAX_ZIP_ENTRY_TEXT_BYTES || bytes.len() as u64 > MAX_ZIP_ENTRY_TEXT_BYTES;
    bytes.truncate(MAX_ZIP_ENTRY_TEXT_BYTES as usize);
    Ok(ZipText {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    })
}

pub(super) fn xml_parse_warning(path: &str, xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth = depth.saturating_add(1),
            Ok(Event::End(_)) if depth > 0 => depth -= 1,
            Ok(Event::End(_)) => {
                return Some(format!(
                    "{path}: XML parse warning: unmatched closing element"
                ));
            }
            Ok(Event::Eof) if depth > 0 => {
                return Some(format!(
                    "{path}: XML parse warning: {depth} unclosed element(s)"
                ));
            }
            Ok(Event::Eof) => return None,
            Err(error) => return Some(format!("{path}: XML parse warning: {error}")),
            _ => {}
        }
    }
}

pub(super) fn resolve_zip_target(base: &str, target: &str) -> Option<String> {
    let target = target.replace('\\', "/");
    if target.contains("://") || target.contains('\0') {
        return None;
    }
    let joined = if target.starts_with('/') {
        target.trim_start_matches('/').to_string()
    } else {
        format!("{base}/{target}")
    };
    let mut parts = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Decode a quick-xml text node and unescape XML entities.
pub(crate) fn office_docx_lines(xml: &str) -> BoundedTextLines {
    tagged_lines(xml, b"w:p", b"w:t")
}

pub(super) fn office_pptx_lines(xml: &str) -> BoundedTextLines {
    tagged_lines(xml, b"a:p", b"a:t")
}

/// Group `<text_tag>` text runs into one line per `<para_tag>` element.
pub(crate) struct BoundedTextLines {
    pub(super) lines: Vec<String>,
    pub(super) truncated: bool,
    pub(super) line_count_truncated: bool,
    pub(super) line_width_truncated: bool,
}

fn tagged_lines(xml: &str, para_tag: &[u8], text_tag: &[u8]) -> BoundedTextLines {
    let mut reader = Reader::from_str(xml);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    let mut line_count_truncated = false;
    let mut line_width_truncated = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if element.name().as_ref() == para_tag => current.clear(),
            Ok(Event::Start(element)) if element.name().as_ref() == text_tag => in_text = true,
            Ok(Event::Text(text)) => {
                if in_text {
                    if let Some(value) = xml_text(&text) {
                        current.push_str(&value);
                    }
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                if in_text {
                    if let Some(value) = resolve_xml_reference(reference.as_ref()) {
                        current.push_str(&value);
                    }
                }
            }
            Ok(Event::End(element)) if element.name().as_ref() == text_tag => in_text = false,
            Ok(Event::End(element)) if element.name().as_ref() == para_tag => {
                let (line, was_truncated) = bounded_line_with_status(&current);
                line_width_truncated |= was_truncated;
                if !line.is_empty() {
                    if lines.len() >= MAX_SECTION_LINES {
                        line_count_truncated = true;
                        break;
                    }
                    lines.push(line);
                }
                current.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    BoundedTextLines {
        lines,
        truncated: line_count_truncated || line_width_truncated,
        line_count_truncated,
        line_width_truncated,
    }
}

pub(crate) fn xml_text(text: &quick_xml::events::BytesText<'_>) -> Option<String> {
    let decoded = text.decode().ok()?;
    quick_xml::escape::unescape(&decoded)
        .ok()
        .map(|value| value.into_owned())
}
/// Resolve a numeric character reference (`&#38;` / `&#x26;`).
pub(crate) fn resolve_xml_reference(reference: &[u8]) -> Option<String> {
    let name = std::str::from_utf8(reference).ok()?;
    if let Some(value) = quick_xml::escape::resolve_predefined_entity(name) {
        return Some(value.to_string());
    }
    let code = name
        .strip_prefix("#x")
        .or_else(|| name.strip_prefix("#X"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .or_else(|| name.strip_prefix('#')?.parse::<u32>().ok())?;
    char::from_u32(code).map(|value| value.to_string())
}
