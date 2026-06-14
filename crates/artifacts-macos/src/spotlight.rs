//! Spotlight index parser.
//!
//! Parses the Spotlight metadata store (typically `.store.db` SQLite databases)
//! found under `/.Spotlight-V100/` on each volume.
//!
//! The Spotlight store is a SQLite database containing indexed file metadata.
//! Key tables include:
//! - `kMDItemStore` — contains `kMDItem*` column values
//! - Various metadata columns for display name, kind, content type, dates, authors, etc.
//!
//! This parser opens the SQLite database and queries metadata columns to extract
//! indexed file information.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A single Spotlight index entry representing an indexed file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpotlightEntry {
    /// Full path of the indexed file
    pub file_path: String,
    /// Display name (filename)
    pub display_name: String,
    /// Kind string (e.g., "PDF document", "JPEG image")
    pub kind: String,
    /// UTI content type (e.g., "com.adobe.pdf")
    pub content_type: String,
    /// ISO 8601 date strings (created, modified, last opened, etc.)
    pub dates: Vec<String>,
    /// Author names
    pub authors: Vec<String>,
}

/// Parse a Spotlight `.store.db` file from raw bytes.
///
/// Opens the SQLite database in memory and queries the metadata tables to extract
/// file entries with their Spotlight attributes.
pub fn parse_spotlight_store(data: &[u8]) -> Result<Vec<SpotlightEntry>, String> {
    if data.is_empty() {
        return Err("Spotlight store data is empty".to_string());
    }

    // Write the data to a temp file and open it as SQLite
    // This avoids the serde Deserialize trait conflict with Connection::deserialize
    use std::io::Write;
    let mut tmp = tempfile::Builder::new()
        .suffix(".store.db")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    let conn = Connection::open(tmp.path())
        .map_err(|e| format!("Failed to open Spotlight store database: {}", e))?;

    let mut entries: Vec<SpotlightEntry> = Vec::new();

    // Try multiple possible table structures for Spotlight metadata
    // Check if kMDItemStore-like table exists
    let tables = get_table_names(&conn)?;

    // Look for tables that might contain Spotlight metadata
    let metadata_tables: Vec<String> = tables
        .into_iter()
        .filter(|t| {
            t.contains("kMD") || t.contains("Item") || t.contains("metadata") || t.contains("store")
        })
        .collect();

    if metadata_tables.is_empty() {
        // Try a generic approach: look for any table with common kMDItem* columns
        return parse_generic_metadata(&conn);
    }

    for table in &metadata_tables {
        if let Ok(table_entries) = parse_metadata_table(&conn, table) {
            entries.extend(table_entries);
        }
    }

    if entries.is_empty() {
        // Fallback to generic parsing
        entries = parse_generic_metadata(&conn)?;
    }

    Ok(entries)
}

/// Get all table names from the database.
fn get_table_names(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .map_err(|e| format!("Failed to prepare table query: {}", e))?;

    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("Failed to query tables: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(names)
}

/// Parse entries from a specific metadata table.
fn parse_metadata_table(conn: &Connection, table: &str) -> Result<Vec<SpotlightEntry>, String> {
    // Get column info for this table
    let mut col_stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| format!("Failed to get table info for {}: {}", table, e))?;

    let columns: Vec<String> = col_stmt
        .query_map([], |row| row.get(1))
        .map_err(|e| format!("Failed to read columns: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    // Look for key Spotlight columns
    let has_path = columns
        .iter()
        .any(|c| c.contains("Path") || c.contains("path"));
    let has_display_name = columns
        .iter()
        .any(|c| c.contains("DisplayName") || c.contains("FSName"));
    let has_kind = columns.iter().any(|c| c.contains("Kind"));
    let has_content_type = columns
        .iter()
        .any(|c| c.contains("ContentType") || c.contains("UTI"));

    if !has_path && !has_display_name {
        return Ok(Vec::new());
    }

    // Build query using only columns that actually exist
    let mut select_cols: Vec<&str> = Vec::new();
    if has_path {
        if let Some(c) = columns
            .iter()
            .find(|c| c.contains("Path") || c.as_str() == "path")
        {
            select_cols.push(c);
        }
    }
    if has_display_name {
        if let Some(c) = columns
            .iter()
            .find(|c| c.contains("DisplayName") || c.contains("FSName"))
        {
            select_cols.push(c);
        }
    }
    if has_kind {
        if let Some(c) = columns.iter().find(|c| c.contains("Kind")) {
            select_cols.push(c);
        }
    }
    if has_content_type {
        if let Some(c) = columns
            .iter()
            .find(|c| c.contains("ContentType") || c.contains("UTI"))
        {
            select_cols.push(c);
        }
    }

    if select_cols.is_empty() {
        select_cols.push("*");
    }

    let query = format!("SELECT {} FROM {} LIMIT 500", select_cols.join(", "), table);

    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| format!("Failed to prepare query for {}: {}", table, e))?;

    let entries: Vec<SpotlightEntry> = stmt
        .query_map([], |row| {
            let file_path: String = row.get(0).unwrap_or_default();
            let display_name: String = if select_cols.len() > 1 {
                row.get(1).unwrap_or_default()
            } else {
                String::new()
            };
            let kind: String = if select_cols.len() > 2 {
                row.get(2).unwrap_or_default()
            } else {
                String::new()
            };
            let content_type: String = if select_cols.len() > 3 {
                row.get(3).unwrap_or_default()
            } else {
                String::new()
            };

            Ok(SpotlightEntry {
                file_path,
                display_name,
                kind,
                content_type,
                dates: Vec::new(),
                authors: Vec::new(),
            })
        })
        .map_err(|e| format!("Failed to query {}: {}", table, e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(entries)
}

/// Fallback: try to extract metadata from any table with recognizable columns.
fn parse_generic_metadata(conn: &Connection) -> Result<Vec<SpotlightEntry>, String> {
    let mut entries: Vec<SpotlightEntry> = Vec::new();

    // Known Spotlight metadata column names
    let path_columns = [
        "kMDItemPath",
        "kMDItemFSName",
        "path",
        "FilePath",
        "file_path",
    ];
    let name_columns = [
        "kMDItemDisplayName",
        "kMDItemFSName",
        "display_name",
        "DisplayName",
        "name",
    ];
    let kind_columns = ["kMDItemKind", "kind", "Kind"];
    let content_columns = [
        "kMDItemContentType",
        "kMDItemContentTypeTree",
        "content_type",
        "UTI",
    ];

    let tables = get_table_names(conn)?;

    for table in &tables {
        // Skip internal SQLite tables
        if table.starts_with("sqlite_") {
            continue;
        }

        let columns = get_column_names(conn, table)?;

        // Find matching columns
        let path_col = find_matching_column(&columns, &path_columns);
        let name_col = find_matching_column(&columns, &name_columns);
        let kind_col = find_matching_column(&columns, &kind_columns);
        let content_col = find_matching_column(&columns, &content_columns);

        if path_col.is_none() && name_col.is_none() {
            continue;
        }

        // Build a query with the available columns
        let mut select_parts: Vec<String> = Vec::new();
        select_parts.push(
            path_col
                .or(name_col)
                .cloned()
                .unwrap_or_else(|| "'unknown'".to_string()),
        );
        select_parts.push(
            name_col
                .or(path_col)
                .cloned()
                .unwrap_or_else(|| "''".to_string()),
        );
        select_parts.push(kind_col.cloned().unwrap_or_else(|| "''".to_string()));
        select_parts.push(content_col.cloned().unwrap_or_else(|| "''".to_string()));

        let query = format!(
            "SELECT {} FROM {} LIMIT 200",
            select_parts.join(", "),
            table
        );

        let mut stmt = match conn.prepare(&query) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let table_entries: Vec<SpotlightEntry> = stmt
            .query_map([], |row| {
                let file_path: String = row.get(0).unwrap_or_default();
                let display_name: String = row.get(1).unwrap_or_default();
                let kind: String = row.get(2).unwrap_or_default();
                let content_type: String = row.get(3).unwrap_or_default();

                Ok(SpotlightEntry {
                    file_path,
                    display_name,
                    kind,
                    content_type,
                    dates: Vec::new(),
                    authors: Vec::new(),
                })
            })
            .map_err(|e| format!("Failed to query {}: {}", table, e))?
            .filter_map(|r| r.ok())
            .collect();

        entries.extend(table_entries);
    }

    Ok(entries)
}

/// Get column names for a table.
fn get_column_names(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| format!("Failed to get column info: {}", e))?;

    let names: Vec<String> = stmt
        .query_map([], |row| row.get(1))
        .map_err(|e| format!("Failed to read columns: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(names)
}

/// Find the first column name that contains any of the candidate substrings.
fn find_matching_column<'a>(columns: &'a [String], candidates: &[&str]) -> Option<&'a String> {
    for candidate in candidates {
        for col in columns {
            if col.contains(candidate) || col.eq_ignore_ascii_case(candidate) {
                return Some(col);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal SQLite database resembling a Spotlight store.
    /// Writes to a temp file and reads back the bytes.
    fn build_spotlight_test_db() -> Vec<u8> {
        let tmp = tempfile::Builder::new()
            .suffix(".spotlight.db")
            .tempfile()
            .expect("create temp file");
        let tmp_path = tmp.path().to_path_buf();
        drop(tmp);

        let conn = Connection::open(&tmp_path).expect("open temp db");

        conn.execute_batch(
            "CREATE TABLE kMDItem (
                kMDItemPath TEXT,
                kMDItemDisplayName TEXT,
                kMDItemKind TEXT,
                kMDItemContentType TEXT,
                kMDItemFSCreationDate REAL,
                kMDItemFSContentChangeDate REAL,
                kMDItemAuthors TEXT
            );

            INSERT INTO kMDItem VALUES
                ('/Users/test/Documents/report.pdf', 'report.pdf', 'PDF document', 'com.adobe.pdf', 696902400.0, 697000000.0, 'John Doe'),
                ('/Users/test/Pictures/photo.jpg', 'photo.jpg', 'JPEG image', 'public.jpeg', 696800000.0, 696900000.0, '');
            ",
        )
        .expect("create test db");

        drop(conn);

        std::fs::read(&tmp_path).expect("read temp db")
    }

    #[test]
    fn parse_empty_data() {
        let result = parse_spotlight_store(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_spotlight_store_extracts_entries() {
        let data = build_spotlight_test_db();
        let entries = parse_spotlight_store(&data).expect("should parse");

        // We should get at least one entry
        assert!(
            !entries.is_empty(),
            "Expected at least one entry from Spotlight DB"
        );

        // First entry should have correct values
        let first = &entries[0];
        assert!(
            !first.display_name.is_empty(),
            "Display name should not be empty"
        );
    }

    #[test]
    fn parse_invalid_db_handles_gracefully() {
        // Random bytes that aren't a valid SQLite database
        let result = parse_spotlight_store(b"This is not a SQLite database file at all");
        // Should error gracefully
        assert!(result.is_err());
    }

    #[test]
    fn get_column_names_works() {
        let conn = Connection::open_in_memory().expect("open");
        conn.execute_batch("CREATE TABLE test (col_a TEXT, col_b INTEGER);")
            .expect("create");

        let cols = get_column_names(&conn, "test").expect("get columns");
        assert_eq!(cols.len(), 2);
        assert!(cols.contains(&"col_a".to_string()));
        assert!(cols.contains(&"col_b".to_string()));
    }
}
