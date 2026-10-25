use crate::{
    history::VisitError,
    sqlite::{table_columns, table_exists},
    BrowserError,
};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct DownloadRow {
    pub row_id: i64,
    pub url: Option<String>,
    pub target_path: Option<String>,
    pub raw_start: Option<i64>,
    pub raw_end: Option<i64>,
    pub total_bytes: Option<i64>,
    pub url_source: &'static str,
}

pub fn visit_downloads<E>(
    connection: &Connection,
    cancel: &AtomicBool,
    mut visit: impl FnMut(DownloadRow) -> std::result::Result<(), E>,
) -> std::result::Result<(), VisitError<E>> {
    if !table_exists(connection, "downloads")? {
        return Ok(());
    }
    let columns = table_columns(connection, "downloads")?;
    let (url, url_source) = source_url(connection, &columns)?;
    let target = column(&columns, &["target_path", "current_path"]);
    let start = column(&columns, &["start_time"]);
    let end = column(&columns, &["end_time"]);
    let bytes = column(&columns, &["total_bytes", "received_bytes"]);
    let mut cursor = i64::MIN;
    let mut first = true;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(BrowserError::Cancelled.into());
        }
        crate::sqlite::refresh_query_budget(connection);
        let op = if first { ">=" } else { ">" };
        let sql = format!("SELECT d.rowid,{url},{target},{start},{end},{bytes} FROM downloads d WHERE d.rowid {op} ?1 ORDER BY d.rowid LIMIT 1024");
        let mut statement = connection
            .prepare_cached(&sql)
            .map_err(BrowserError::from)?;
        let mut rows = statement.query([cursor]).map_err(BrowserError::from)?;
        let mut count = 0;
        while let Some(row) = rows.next().map_err(BrowserError::from)? {
            if cancel.load(Ordering::Acquire) {
                return Err(BrowserError::Cancelled.into());
            }
            let record = decode(row, url_source).map_err(BrowserError::from)?;
            cursor = record.row_id;
            visit(record).map_err(VisitError::Sink)?;
            count += 1;
        }
        if count < 1024 || cursor == i64::MAX {
            return Ok(());
        }
        first = false;
    }
}

fn column(columns: &[String], choices: &[&str]) -> String {
    choices
        .iter()
        .find(|choice| columns.iter().any(|column| column == **choice))
        .map(|column| format!("d.{column}"))
        .unwrap_or_else(|| "NULL".into())
}

fn source_url(
    connection: &Connection,
    columns: &[String],
) -> crate::Result<(String, &'static str)> {
    if columns.iter().any(|column| column == "id")
        && table_exists(connection, "downloads_url_chains")?
    {
        let chain = table_columns(connection, "downloads_url_chains")?;
        if ["id", "chain_index", "url"]
            .iter()
            .all(|name| chain.iter().any(|column| column == name))
        {
            return Ok(("(SELECT c.url FROM downloads_url_chains c WHERE c.id=d.id ORDER BY c.chain_index DESC LIMIT 1)".into(), "downloads_url_chains.url"));
        }
        return Err(BrowserError::Unsupported("download URL chain columns"));
    }
    if columns.iter().any(|column| column == "url") {
        Ok(("d.url".into(), "downloads.url"))
    } else if columns.iter().any(|column| column == "tab_url") {
        Ok(("d.tab_url".into(), "downloads.tab_url"))
    } else {
        Ok(("NULL".into(), "unavailable"))
    }
}

fn decode(row: &rusqlite::Row<'_>, url_source: &'static str) -> rusqlite::Result<DownloadRow> {
    Ok(DownloadRow {
        row_id: row.get(0)?,
        url: row.get(1)?,
        target_path: row.get(2)?,
        raw_start: row.get(3)?,
        raw_end: row.get(4)?,
        total_bytes: row.get(5)?,
        url_source,
    })
}
