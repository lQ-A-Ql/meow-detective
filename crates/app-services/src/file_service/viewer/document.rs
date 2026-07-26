//! Structured preview for document-like evidence files: PDF, Office Open XML
//! (docx/xlsx/pptx), and SQLite databases. Output is a bounded text
//! extraction, never a layout render or inlined binary payload.

use rusqlite::{serialize::OwnedData, Connection, DatabaseName};
use std::ptr::NonNull;
use transport::dto::{DocumentPreviewDto, DocumentSectionDto, DocumentTableDto};

use super::office_preview::{office_docx_lines, preview_office_part, preview_pptx, preview_xlsx};
use super::{
    open_file_handle_real, preview_bytes::read_inline_preview_bytes_for_file, PreviewReadContext,
};
use crate::file_service::FileServiceError;

const MAX_DOCUMENT_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDF_PAGES: usize = 10;
pub(crate) const MAX_SECTION_LINES: usize = 200;
pub(crate) const MAX_LINE_CHARS: usize = 300;
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
            table: None,
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
    let conn = open_sqlite_in_memory(bytes)?;
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
    let mut truncated = tables.len() > MAX_SQLITE_TABLES;
    let tables = &tables[..tables.len().min(MAX_SQLITE_TABLES)];

    let mut sections = Vec::new();
    let mut warnings = Vec::new();
    for table in tables {
        match sqlite_table_section(&conn, table) {
            Ok((section, rows_truncated)) => {
                truncated |= rows_truncated;
                if rows_truncated {
                    warnings.push(format!(
                        "table {table}: row preview limited to {MAX_SQLITE_ROWS} rows"
                    ));
                }
                sections.push(section);
            }
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

fn open_sqlite_in_memory(bytes: &[u8]) -> Result<Connection, FileServiceError> {
    const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";
    if bytes.len() < 100 || !bytes.starts_with(SQLITE_HEADER) {
        return Err(FileServiceError::invalid_input("SQLite header is invalid"));
    }
    let allocation_size = u64::try_from(bytes.len())
        .map_err(|_| FileServiceError::invalid_input("SQLite preview is too large"))?;
    // SAFETY: sqlite3_malloc64 returns SQLite-owned storage suitable for
    // OwnedData. The checked non-null allocation is initialized with exactly
    // bytes.len() bytes before ownership is transferred to rusqlite.
    let allocation = unsafe { rusqlite::ffi::sqlite3_malloc64(allocation_size) }.cast::<u8>();
    let allocation = NonNull::new(allocation)
        .ok_or_else(|| FileServiceError::invalid_input("SQLite memory allocation failed"))?;
    // SAFETY: source and destination are valid for bytes.len() bytes and do
    // not overlap; the destination was allocated immediately above.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), allocation.as_ptr(), bytes.len());
    }
    // SAFETY: allocation came from sqlite3_malloc64 and ownership is moved
    // exactly once into OwnedData.
    let data = unsafe { OwnedData::from_raw_nonnull(allocation, bytes.len()) };
    let mut connection = Connection::open_in_memory().map_err(sqlite_error)?;
    connection
        .deserialize(DatabaseName::Main, data, true)
        .map_err(sqlite_error)?;
    Ok(connection)
}

fn sqlite_table_section(
    conn: &Connection,
    table: &str,
) -> Result<(DocumentSectionDto, bool), rusqlite::Error> {
    let quoted = format!("\"{}\"", table.replace('"', "\"\""));
    let columns = conn
        .prepare(&format!("PRAGMA table_info({quoted})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut lines = vec![columns.join(" | ")];
    let mut statement = conn.prepare(&format!("SELECT * FROM {quoted} LIMIT ?1"))?;
    let width = columns.len();
    let rows = statement.query_map([(MAX_SQLITE_ROWS + 1) as i64], |row| {
        let cells = (0..width)
            .map(|index| match row.get_ref(index) {
                Ok(value) => bounded_line(&render_sqlite_value(value)),
                Err(_) => "?".to_string(),
            })
            .collect::<Vec<_>>();
        Ok(cells)
    })?;
    let mut grid = Vec::new();
    for row in rows {
        let cells = row?;
        lines.push(bounded_line(&cells.join(" | ")));
        grid.push(cells);
    }
    let truncated = grid.len() > MAX_SQLITE_ROWS;
    grid.truncate(MAX_SQLITE_ROWS);
    lines.truncate(MAX_SQLITE_ROWS + 1);
    Ok((
        DocumentSectionDto {
            title: format!("Table: {table}"),
            lines,
            table: Some(DocumentTableDto {
                columns,
                rows: grid,
            }),
        },
        truncated,
    ))
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
    bounded_line_with_status(line).0
}

pub(crate) fn bounded_line_with_status(line: &str) -> (String, bool) {
    let mut characters = line.trim().chars();
    let bounded = characters.by_ref().take(MAX_LINE_CHARS).collect();
    (bounded, characters.next().is_some())
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/document.rs"]
mod tests;
