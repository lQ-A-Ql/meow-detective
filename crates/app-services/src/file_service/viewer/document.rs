//! Structured preview for document-like evidence files: PDF, Office Open XML
//! (docx/xlsx/pptx), and SQLite databases. Output is a bounded text
//! extraction, never a layout render or inlined binary payload.

use rusqlite::Connection;
use std::io::Write;
use transport::dto::{DocumentPreviewDto, DocumentSectionDto};

use super::office_preview::{office_docx_lines, preview_office_part, preview_pptx, preview_xlsx};
use super::{
    open_file_handle_real, preview_bytes::read_inline_preview_bytes_for_file, PreviewReadContext,
};
use crate::file_service::FileServiceError;

const MAX_DOCUMENT_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDF_PAGES: usize = 10;
pub(crate) const MAX_SECTION_LINES: usize = 200;
const MAX_LINE_CHARS: usize = 300;
const MAX_SQLITE_TABLES: usize = 20;
const MAX_SQLITE_ROWS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentKind {
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

    pub(crate) fn as_str(self) -> &'static str {
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
        DocumentKind::Pptx => preview_pptx(&bytes),
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

fn preview_sqlite(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut tmp = tempfile::NamedTempFile::new().map_err(FileServiceError::from)?;
    tmp.write_all(bytes).map_err(FileServiceError::from)?;
    tmp.flush().map_err(FileServiceError::from)?;
    let conn = Connection::open_with_flags(tmp.path(), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| FileServiceError::invalid_input(format!("SQLite open failed: {error}")))?;
    let mut statement = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name LIMIT ?1",
        )
        .map_err(sqlite_error)?;
    let tables = statement
        .query_map([(MAX_SQLITE_TABLES + 1) as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(sqlite_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_error)?;
    let truncated = tables.len() > MAX_SQLITE_TABLES;
    let tables = &tables[..tables.len().min(MAX_SQLITE_TABLES)];

    let mut sections = Vec::new();
    let mut warnings = Vec::new();
    for table in tables {
        match sqlite_table_section(&conn, table) {
            Ok(section) => sections.push(section),
            Err(error) => warnings.push(format!("table {table}: {error}")),
        }
    }
    Ok(DocumentPreviewDto {
        kind: DocumentKind::Sqlite.as_str().to_string(),
        summary: format!("{} tables", tables.len()),
        sections,
        truncated,
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
    FileServiceError::invalid_input(format!("sqlite parse error: {error}"))
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

pub(crate) fn bounded_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(MAX_SECTION_LINES)
        .map(bounded_line)
        .collect()
}

pub(crate) fn bounded_line(line: &str) -> String {
    line.trim().chars().take(MAX_LINE_CHARS).collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/document.rs"]
mod tests;
