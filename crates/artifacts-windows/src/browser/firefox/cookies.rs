use super::sqlite::{open_sqlite_from_bytes, table_exists};
use super::time::unix_seconds_to_dt;
use crate::browser::chromium::{BrowserCookie, BrowserDecryptionStatus};
use rusqlite::params;

pub fn parse_firefox_cookies(data: &[u8]) -> Result<Vec<BrowserCookie>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;
    if !table_exists(&conn, "moz_cookies") {
        return Ok(Vec::new());
    }

    let columns = cookie_columns(&conn)?;
    let sql = cookie_query(&columns);
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("prepare cookies query: {}", e))?;
    let rows = stmt
        .query_map(params![], |row| {
            let raw_value: Option<String> = row.get(2).ok();
            let value_preview = raw_value.as_deref().and_then(cookie_value_preview);
            let decryption_status = if value_preview.is_some() {
                BrowserDecryptionStatus::Plaintext
            } else {
                BrowserDecryptionStatus::Unavailable
            };
            Ok(BrowserCookie {
                domain: row.get(0)?,
                name: row.get(1)?,
                value_preview,
                expiry: unix_seconds_to_dt(row.get(3).unwrap_or(0)),
                secure: row
                    .get::<_, i64>(4)
                    .map(|value| value != 0)
                    .unwrap_or(false),
                http_only: row
                    .get::<_, i64>(5)
                    .map(|value| value != 0)
                    .unwrap_or(false),
                same_site: row.get(6).ok(),
                decryption_status,
                decryption_detail: (decryption_status == BrowserDecryptionStatus::Unavailable)
                    .then(|| "Firefox NSS decryption is not configured".to_string()),
            })
        })
        .map_err(|e| format!("query cookies: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(cookie) => results.push(cookie),
            Err(err) => tracing::warn!("skipping firefox cookie row: {}", err),
        }
    }
    Ok(results)
}

fn cookie_columns(conn: &rusqlite::Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(moz_cookies)")
        .map_err(|e| format!("pragma: {}", e))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("pragma rows: {}", e))?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row.map_err(|e| format!("pragma row: {}", e))?);
    }
    Ok(columns)
}

fn cookie_query(columns: &[String]) -> String {
    let has = |name: &str| columns.iter().any(|column| column == name);
    let mut selected = vec![
        "baseDomain".to_string(),
        "name".to_string(),
        "value".to_string(),
        "expiry".to_string(),
    ];
    selected.push(if has("isSecure") {
        "isSecure".to_string()
    } else {
        "0 AS isSecure".to_string()
    });
    selected.push(if has("isHttpOnly") {
        "isHttpOnly".to_string()
    } else {
        "0 AS isHttpOnly".to_string()
    });
    selected.push(if has("sameSite") {
        "sameSite".to_string()
    } else {
        "NULL AS sameSite".to_string()
    });
    format!(
        "SELECT {} FROM moz_cookies ORDER BY baseDomain",
        selected.join(", ")
    )
}

fn cookie_value_preview(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else if is_likely_encrypted(value) {
        Some(format!("[encrypted {} bytes]", value.len()))
    } else {
        Some(value.chars().take(128).collect())
    }
}

pub(super) fn is_likely_encrypted(value: &str) -> bool {
    if value.len() < 8 {
        return false;
    }
    let non_printable = value
        .as_bytes()
        .iter()
        .filter(|&&byte| !(0x20..=0x7e).contains(&byte))
        .count();
    (non_printable as f64) > (value.len() as f64 * 0.3)
}
