//! Structured preview for document-like evidence files: PDF, Office Open XML
//! (docx/xlsx/pptx), and SQLite databases. Output is a bounded text
//! extraction, never a layout render or inlined binary payload.

use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::Connection;
use std::io::{Cursor, Read, Write};
use transport::dto::{DocumentPreviewDto, DocumentSectionDto};
use zip::ZipArchive;

use super::{
    open_file_handle_real, preview_bytes::read_inline_preview_bytes_for_file, PreviewReadContext,
};
use crate::file_service::FileServiceError;

const MAX_DOCUMENT_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDF_PAGES: usize = 10;
const MAX_OFFICE_PARTS: usize = 20;
const MAX_SECTION_LINES: usize = 200;
const MAX_LINE_CHARS: usize = 300;
const MAX_SQLITE_TABLES: usize = 20;
const MAX_SQLITE_ROWS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentKind {
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Sqlite,
}

impl DocumentKind {
    fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "application/pdf" => Some(Self::Pdf),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Some(Self::Docx)
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some(Self::Xlsx),
            "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
                Some(Self::Pptx)
            }
            "application/x-sqlite3" => Some(Self::Sqlite),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Sqlite => "sqlite",
        }
    }
}

/// Build a structured document preview for a file entry.
pub fn document_preview_for_file<C>(
    mut context: C,
    file_id: &str,
) -> Result<DocumentPreviewDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;
    let kind = DocumentKind::from_mime(handle.mime.as_deref().unwrap_or("")).ok_or_else(|| {
        FileServiceError::invalid_input("Not a document-like file (PDF, Office, or SQLite)")
    })?;
    if handle.size == 0 {
        return Err(FileServiceError::invalid_input("Empty file"));
    }
    if handle.size > MAX_DOCUMENT_PREVIEW_BYTES {
        return Err(FileServiceError::invalid_input(format!(
            "Document preview is limited to {} MB",
            MAX_DOCUMENT_PREVIEW_BYTES / (1024 * 1024)
        )));
    }
    let bytes = read_inline_preview_bytes_for_file(&mut context, file_id, handle.size)?;
    match kind {
        DocumentKind::Pdf => preview_pdf(&bytes),
        DocumentKind::Docx => preview_office_part(&bytes, kind, office_docx_lines),
        DocumentKind::Xlsx => preview_xlsx(&bytes),
        DocumentKind::Pptx => preview_office_part(&bytes, kind, office_pptx_lines),
        DocumentKind::Sqlite => preview_sqlite(&bytes),
    }
}

fn preview_pdf(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let doc = lopdf::Document::load_mem(bytes)
        .map_err(|error| FileServiceError::invalid_input(format!("PDF parse failed: {error}")))?;
    let page_numbers = doc.get_pages().keys().copied().collect::<Vec<_>>();
    let mut sections = Vec::new();
    for (index, page_number) in page_numbers.iter().take(MAX_PDF_PAGES).enumerate() {
        let text = doc.extract_text(&[*page_number]).unwrap_or_default();
        sections.push(DocumentSectionDto {
            title: format!("Page {}", index + 1),
            lines: bounded_lines(&text),
        });
    }
    let truncated = page_numbers.len() > MAX_PDF_PAGES;
    Ok(DocumentPreviewDto {
        kind: DocumentKind::Pdf.as_str().to_string(),
        summary: format!("{} pages", page_numbers.len()),
        sections,
        truncated,
        warnings: Vec::new(),
    })
}

fn preview_office_part(
    bytes: &[u8],
    kind: DocumentKind,
    extract: impl Fn(&str) -> Vec<String>,
) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("Office container parse failed: {error}"))
    })?;
    let (part_path, part_name) = match kind {
        DocumentKind::Docx => ("word/document.xml", "Document"),
        DocumentKind::Pptx => ("ppt/presentation.xml", "Presentation"),
        _ => unreachable!("preview_office_part only handles docx/pptx"),
    };
    let _ = part_name;
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

fn preview_xlsx(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("XLSX container parse failed: {error}"))
    })?;
    let sheet_names = xlsx_sheet_names(&read_zip_text(&mut archive, "xl/workbook.xml")?);
    let shared = read_zip_text(&mut archive, "xl/sharedStrings.xml")
        .map(|xml| xlsx_shared_strings(&xml))
        .unwrap_or_default();
    let mut sections = Vec::new();
    let mut warnings = Vec::new();
    for (index, name) in sheet_names.iter().take(MAX_OFFICE_PARTS).enumerate() {
        let path = format!("xl/worksheets/sheet{}.xml", index + 1);
        match read_zip_text(&mut archive, &path) {
            Ok(xml) => sections.push(DocumentSectionDto {
                title: format!("Sheet: {name}"),
                lines: xlsx_sheet_lines(&xml, &shared),
            }),
            Err(error) => warnings.push(format!("{path}: {error}")),
        }
    }
    let truncated = sheet_names.len() > MAX_OFFICE_PARTS;
    Ok(DocumentPreviewDto {
        kind: DocumentKind::Xlsx.as_str().to_string(),
        summary: format!("{} sheets", sheet_names.len()),
        sections,
        truncated,
        warnings,
    })
}

fn preview_sqlite(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut tmp = tempfile::NamedTempFile::new().map_err(FileServiceError::from)?;
    tmp.write_all(bytes).map_err(FileServiceError::from)?;
    tmp.flush().map_err(FileServiceError::from)?;
    let conn = Connection::open(tmp.path())
        .map_err(|error| FileServiceError::invalid_input(format!("SQLite open failed: {error}")))?;
    let mut statement = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name LIMIT ?1",
        )
        .map_err(sqlite_error)?;
    let tables = statement
        .query_map([MAX_SQLITE_TABLES as i64], |row| row.get::<_, String>(0))
        .map_err(sqlite_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_error)?;

    let mut sections = Vec::new();
    let mut warnings = Vec::new();
    for table in &tables {
        match sqlite_table_section(&conn, table) {
            Ok(section) => sections.push(section),
            Err(error) => warnings.push(format!("table {table}: {error}")),
        }
    }
    Ok(DocumentPreviewDto {
        kind: DocumentKind::Sqlite.as_str().to_string(),
        summary: format!("{} tables", tables.len()),
        sections,
        truncated: tables.len() >= MAX_SQLITE_TABLES,
        warnings,
    })
}

fn sqlite_table_section(
    conn: &Connection,
    table: &str,
) -> Result<DocumentSectionDto, rusqlite::Error> {
    let quoted = format!("\"{}\"", table.replace('"', "\"\""));
    let columns = conn
        .prepare(&format!("PRAGMA table_info({quoted})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut lines = vec![columns.join(" | ")];
    let mut statement = conn.prepare(&format!("SELECT * FROM {quoted} LIMIT ?1"))?;
    let width = columns.len();
    let rows = statement.query_map([MAX_SQLITE_ROWS as i64], |row| {
        let cells = (0..width)
            .map(|index| match row.get_ref(index) {
                Ok(value) => render_sqlite_value(value),
                Err(_) => "?".to_string(),
            })
            .collect::<Vec<_>>();
        Ok(cells.join(" | "))
    })?;
    for row in rows {
        lines.push(bounded_line(&row?));
        if lines.len() > MAX_SQLITE_ROWS {
            break;
        }
    }
    Ok(DocumentSectionDto {
        title: format!("Table: {table}"),
        lines,
    })
}

fn sqlite_error(error: rusqlite::Error) -> FileServiceError {
    FileServiceError::Other(format!("sqlite error: {error}"))
}

fn render_sqlite_value(value: rusqlite::types::ValueRef<'_>) -> String {
    match value {
        rusqlite::types::ValueRef::Null => "NULL".to_string(),
        rusqlite::types::ValueRef::Integer(value) => value.to_string(),
        rusqlite::types::ValueRef::Real(value) => value.to_string(),
        rusqlite::types::ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned(),
        rusqlite::types::ValueRef::Blob(value) => format!("[blob {} bytes]", value.len()),
    }
}

fn read_zip_text(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
) -> Result<String, FileServiceError> {
    let mut entry = archive
        .by_name(path)
        .map_err(|error| FileServiceError::invalid_input(format!("{path}: {error}")))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(FileServiceError::from)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Decode a quick-xml text node and unescape XML entities.
fn xml_text(text: &quick_xml::events::BytesText<'_>) -> Option<String> {
    let decoded = text.decode().ok()?;
    quick_xml::escape::unescape(&decoded)
        .ok()
        .map(|value| value.into_owned())
}

/// Resolve a numeric character reference (`&#38;` / `&#x26;`).
fn resolve_char_reference(name: &str) -> Option<&'static str> {
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

fn office_docx_lines(xml: &str) -> Vec<String> {
    tagged_lines(xml, b"w:p", b"w:t")
}

fn office_pptx_lines(xml: &str) -> Vec<String> {
    tagged_lines(xml, b"a:p", b"a:t")
}

/// Group `<text_tag>` text runs into one line per `<para_tag>` element.
fn tagged_lines(xml: &str, para_tag: &[u8], text_tag: &[u8]) -> Vec<String> {
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

fn xlsx_sheet_names(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut names = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) | Ok(Event::Start(element))
                if element.name().as_ref() == b"sheet" =>
            {
                for attribute in element.attributes().flatten() {
                    if attribute.key.as_ref() == b"name" {
                        names.push(String::from_utf8_lossy(&attribute.value).into_owned());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    names
}

fn xlsx_shared_strings(xml: &str) -> Vec<String> {
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

fn xlsx_sheet_lines(xml: &str, shared: &[String]) -> Vec<String> {
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

fn bounded_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(MAX_SECTION_LINES)
        .map(bounded_line)
        .collect()
}

fn bounded_line(line: &str) -> String {
    line.trim().chars().take(MAX_LINE_CHARS).collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/document.rs"]
mod tests;
