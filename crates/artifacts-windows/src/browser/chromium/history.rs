use super::sqlite::open_sqlite_from_bytes;
use super::time::webkit_time_to_dt;
use super::types::BrowserVisit;
use rusqlite::params;

/// Parse a Chromium `History` SQLite database.
///
/// Queries the `urls` and `visits` tables and returns one `BrowserVisit` for
/// each visit row.
pub fn parse_chrome_history(
    data: &[u8],
    browser: &str,
    profile: Option<&str>,
) -> Result<Vec<BrowserVisit>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn
        .prepare(
            "SELECT u.url, u.title, v.visit_time, u.visit_count
             FROM urls u
             JOIN visits v ON u.id = v.url
             ORDER BY v.visit_time DESC",
        )
        .map_err(|e| format!("prepare history query: {}", e))?;

    let rows = stmt
        .query_map(params![], |row| {
            let url: String = row.get(0)?;
            let title: Option<String> = row.get(1).ok();
            let visit_time_raw: i64 = row.get(2).unwrap_or(0);
            let visit_count: i64 = row.get(3).unwrap_or(1);
            Ok(BrowserVisit {
                url,
                title,
                visit_time: webkit_time_to_dt(visit_time_raw),
                visit_count,
                browser: browser.to_string(),
                profile: profile.map(|s| s.to_string()),
            })
        })
        .map_err(|e| format!("query history: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(visit) => results.push(visit),
            Err(e) => tracing::warn!("skipping history row: {}", e),
        }
    }

    Ok(results)
}
