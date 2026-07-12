use super::sqlite::open_sqlite_from_bytes;
use super::time::webkit_time_to_dt;
use super::types::BrowserPassword;
use rusqlite::params;

/// Parse a Chromium `Login Data` SQLite database.
///
/// The `password_value` column is DPAPI-encrypted and is surfaced only as an
/// `[encrypted N bytes]` preview.
pub fn parse_chrome_passwords(
    data: &[u8],
    browser: &str,
    profile: Option<&str>,
) -> Result<Vec<BrowserPassword>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn
        .prepare(
            "SELECT origin_url, username_value, password_value,
                    date_created, times_used
             FROM logins
             ORDER BY date_created DESC",
        )
        .map_err(|e| format!("prepare passwords query: {}", e))?;

    let rows = stmt
        .query_map(params![], |row| {
            let url: String = row.get(0)?;
            let username: String = row.get(1).unwrap_or_default();
            let password_value: Option<Vec<u8>> = row.get(2).ok();
            let date_created: i64 = row.get(3).unwrap_or(0);
            let times_used: i64 = row.get(4).unwrap_or(0);
            let password_preview = password_value
                .as_ref()
                .map(|bytes| format!("[encrypted {} bytes]", bytes.len()));

            Ok(BrowserPassword {
                url,
                username,
                password_preview,
                created_at: webkit_time_to_dt(date_created),
                times_used: times_used.max(0),
                browser: browser.to_string(),
                profile: profile.map(|s| s.to_string()),
            })
        })
        .map_err(|e| format!("query passwords: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(password) => results.push(password),
            Err(e) => tracing::warn!("skipping password row: {}", e),
        }
    }

    Ok(results)
}
