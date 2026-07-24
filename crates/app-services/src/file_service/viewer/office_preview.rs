//! Office Open XML (docx/xlsx/pptx) structured preview: bounded text
//! extraction from zip container parts.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::{Cursor, Read};
use transport::dto::{DocumentPreviewDto, DocumentSectionDto};
use zip::ZipArchive;

use super::document::{bounded_line, DocumentKind, MAX_SECTION_LINES};
use crate::file_service::FileServiceError;

const MAX_OFFICE_PARTS: usize = 20;

const MAX_ZIP_ENTRY_TEXT_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) fn preview_office_part(
    bytes: &[u8],
    kind: DocumentKind,
    extract: impl Fn(&str) -> Vec<String>,
) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("Office container parse failed: {error}"))
    })?;
    let part_path = match kind {
        DocumentKind::Docx => "word/document.xml",
        _ => unreachable!("preview_office_part only handles docx"),
    };
    let xml = read_zip_text(&mut archive, part_path)?;
    let lines = extract(&xml);
    let truncated = lines.len() >= MAX_SECTION_LINES;
    Ok(DocumentPreviewDto {
        kind: kind.as_str().to_string(),
        summary: format!("{} paragraphs", lines.len()),
        sections: vec![DocumentSectionDto {
            title: "Content".to_string(),
            lines,
        }],
        truncated,
        warnings: Vec::new(),
    })
}

/// `ppt/presentation.xml` only holds the slide-id list and deck defaults.
pub(crate) fn preview_pptx(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("PPTX container parse failed: {error}"))
    })?;
    let mut sections = Vec::new();
    let mut missing = 0usize;
    for index in 1..=MAX_OFFICE_PARTS {
        let path = format!("ppt/slides/slide{index}.xml");
        match read_zip_text(&mut archive, &path) {
            Ok(xml) => sections.push(DocumentSectionDto {
                title: format!("Slide {index}"),
                lines: office_pptx_lines(&xml),
            }),
            Err(_) => {
                missing += 1;
                // Slide parts are numbered contiguously from 1; stop after the
                // first gap once at least one slide was read.
                if !sections.is_empty() {
                    break;
                }
                if missing >= 3 {
                    break;
                }
            }
        }
    }
    let truncated = sections.len() >= MAX_OFFICE_PARTS;
    Ok(DocumentPreviewDto {
        kind: DocumentKind::Pptx.as_str().to_string(),
        summary: format!("{} slides", sections.len()),
        sections,
        truncated,
        warnings: Vec::new(),
    })
}

pub(crate) fn preview_xlsx(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("XLSX container parse failed: {error}"))
    })?;
    let sheets = xlsx_sheet_entries(&read_zip_text(&mut archive, "xl/workbook.xml")?);
    let rels = read_zip_text(&mut archive, "xl/_rels/workbook.xml.rels")
        .map(|xml| xlsx_rels_targets(&xml))
        .unwrap_or_default();
    let shared = read_zip_text(&mut archive, "xl/sharedStrings.xml")
        .map(|xml| xlsx_shared_strings(&xml))
        .unwrap_or_default();
    let mut sections = Vec::new();
    let mut warnings = Vec::new();
    for (index, (name, relationship_id)) in sheets.iter().take(MAX_OFFICE_PARTS).enumerate() {
        // The r:id -> target mapping is authoritative: reordered or deleted
        // sheets make `sheetN.xml` guesses wrong on real workbooks.
        let path = relationship_id
            .as_deref()
            .and_then(|rid| rels.get(rid))
            .map(|target| format!("xl/{target}"))
            .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", index + 1));
        match read_zip_text(&mut archive, &path) {
            Ok(xml) => sections.push(DocumentSectionDto {
                title: format!("Sheet: {name}"),
                lines: xlsx_sheet_lines(&xml, &shared),
            }),
            Err(error) => warnings.push(format!("{path}: {error}")),
        }
    }
    let truncated = sheets.len() > MAX_OFFICE_PARTS;
    Ok(DocumentPreviewDto {
        kind: DocumentKind::Xlsx.as_str().to_string(),
        summary: format!("{} sheets", sheets.len()),
        sections,
        truncated,
        warnings,
    })
}

pub(crate) fn read_zip_text(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
) -> Result<String, FileServiceError> {
    let entry = archive
        .by_name(path)
        .map_err(|error| FileServiceError::invalid_input(format!("{path}: {error}")))?;
    let mut bytes = Vec::with_capacity(entry.size().min(MAX_ZIP_ENTRY_TEXT_BYTES) as usize);
    entry
        .take(MAX_ZIP_ENTRY_TEXT_BYTES)
        .read_to_end(&mut bytes)
        .map_err(FileServiceError::from)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Decode a quick-xml text node and unescape XML entities.
pub(crate) fn office_docx_lines(xml: &str) -> Vec<String> {
    tagged_lines(xml, b"w:p", b"w:t")
}

pub(crate) fn office_pptx_lines(xml: &str) -> Vec<String> {
    tagged_lines(xml, b"a:p", b"a:t")
}

/// Group `<text_tag>` text runs into one line per `<para_tag>` element.
pub(crate) fn tagged_lines(xml: &str, para_tag: &[u8], text_tag: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
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
                    if let Ok(name) = std::str::from_utf8(&reference) {
                        let resolved = resolve_char_reference(name)
                            .or_else(|| quick_xml::escape::resolve_predefined_entity(name));
                        if let Some(value) = resolved {
                            current.push_str(value);
                        }
                    }
                }
            }
            Ok(Event::End(element)) if element.name().as_ref() == text_tag => in_text = false,
            Ok(Event::End(element)) if element.name().as_ref() == para_tag => {
                let line = bounded_line(&current);
                if !line.is_empty() {
                    lines.push(line);
                }
                current.clear();
                if lines.len() >= MAX_SECTION_LINES {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        if lines.len() >= MAX_SECTION_LINES {
            break;
        }
    }
    lines
}

/// Sheet entries from `xl/workbook.xml`: `(name, r:id)`.
/// `Id -> Target` map from `xl/_rels/workbook.xml.rels`.
pub(crate) fn xlsx_shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if element.name().as_ref() == b"si" => {
                in_si = true;
                current.clear();
            }
            Ok(Event::Text(text)) if in_si => {
                if let Some(value) = xml_text(&text) {
                    current.push_str(&value);
                }
            }
            Ok(Event::End(element)) if element.name().as_ref() == b"si" => {
                strings.push(std::mem::take(&mut current));
                in_si = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    strings
}

pub(crate) fn xlsx_sheet_lines(xml: &str, shared: &[String]) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut lines = Vec::new();
    let mut cells = Vec::new();
    let mut cell_type = String::new();
    let mut cell_value = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match element.name().as_ref() {
                b"row" => cells.clear(),
                b"c" => {
                    cell_value.clear();
                    cell_type.clear();
                    for attribute in element.attributes().flatten() {
                        if attribute.key.as_ref() == b"t" {
                            cell_type = String::from_utf8_lossy(&attribute.value).into_owned();
                        }
                    }
                }
                b"v" | b"t" => {}
                _ => {}
            },
            Ok(Event::Text(text)) => {
                if let Some(value) = xml_text(&text) {
                    cell_value.push_str(&value);
                }
            }
            Ok(Event::End(element)) => match element.name().as_ref() {
                b"c" => {
                    let rendered = if cell_type == "s" {
                        cell_value
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| shared.get(index))
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        bounded_line(&cell_value)
                    };
                    cells.push(rendered);
                    cell_value.clear();
                    cell_type.clear();
                }
                b"row" => {
                    if cells.iter().any(|cell| !cell.is_empty()) {
                        lines.push(bounded_line(&cells.join(" | ")));
                    }
                    if lines.len() >= MAX_SECTION_LINES {
                        break;
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    lines
}
pub(crate) fn xml_text(text: &quick_xml::events::BytesText<'_>) -> Option<String> {
    let decoded = text.decode().ok()?;
    quick_xml::escape::unescape(&decoded)
        .ok()
        .map(|value| value.into_owned())
}
/// Resolve a numeric character reference (`&#38;` / `&#x26;`).
pub(crate) fn xlsx_sheet_entries(xml: &str) -> Vec<(String, Option<String>)> {
    let mut reader = Reader::from_str(xml);
    let mut entries = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) | Ok(Event::Start(element))
                if element.name().as_ref() == b"sheet" =>
            {
                let mut name = String::new();
                let mut relationship_id = None;
                for attribute in element.attributes().flatten() {
                    match attribute.key.as_ref() {
                        b"name" => name = String::from_utf8_lossy(&attribute.value).into_owned(),
                        b"r:id" => {
                            relationship_id =
                                Some(String::from_utf8_lossy(&attribute.value).into_owned())
                        }
                        _ => {}
                    }
                }
                entries.push((name, relationship_id));
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    entries
}

pub(crate) fn xlsx_rels_targets(xml: &str) -> std::collections::HashMap<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut targets = std::collections::HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) | Ok(Event::Start(element))
                if element.name().as_ref() == b"Relationship" =>
            {
                let mut id = None;
                let mut target = None;
                for attribute in element.attributes().flatten() {
                    match attribute.key.as_ref() {
                        b"Id" => id = Some(String::from_utf8_lossy(&attribute.value).into_owned()),
                        b"Target" => {
                            target = Some(String::from_utf8_lossy(&attribute.value).into_owned())
                        }
                        _ => {}
                    }
                }
                if let (Some(id), Some(target)) = (id, target) {
                    targets.insert(id, target);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    targets
}

pub(crate) fn resolve_char_reference(name: &str) -> Option<&'static str> {
    let code = name
        .strip_prefix("#x")
        .or_else(|| name.strip_prefix("#X"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .or_else(|| name.strip_prefix('#')?.parse::<u32>().ok())?;
    // Only the XML-significant ASCII references are returned statically;
    // everything else is left to the caller's plain-text handling.
    match code {
        38 => Some("&"),
        60 => Some("<"),
        62 => Some(">"),
        34 => Some("\""),
        39 => Some("'"),
        _ => None,
    }
}
