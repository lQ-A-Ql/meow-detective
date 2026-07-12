use super::sqlite::{open_sqlite_from_bytes, table_exists};
use super::time::firefox_time_to_dt;
use crate::browser::chromium::BrowserVisit;
use rusqlite::params;

pub fn parse_firefox_history(data: &[u8]) -> Result<Vec<BrowserVisit>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;
    if !table_exists(&conn, "moz_places") {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            "SELECT p.url, p.title, p.visit_count, p.last_visit_date,
                    v.visit_date
             FROM moz_places p
             LEFT JOIN moz_historyvisits v ON v.place_id = p.id
             ORDER BY COALESCE(v.visit_date, p.last_visit_date) DESC",
        )
        .map_err(|e| format!("prepare firefox history query: {}", e))?;
    let rows = stmt
        .query_map(params![], |row| {
            let visit_date: Option<i64> = row.get(4).ok();
            let last_visit: Option<i64> = row.get(3).ok();
            Ok(BrowserVisit {
                url: row.get(0)?,
                title: row.get(1).ok(),
                visit_time: visit_date.or(last_visit).and_then(firefox_time_to_dt),
                visit_count: row.get(2).unwrap_or(0),
                browser: "Firefox".to_string(),
                profile: None,
            })
        })
        .map_err(|e| format!("query firefox history: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(visit) => results.push(visit),
            Err(err) => tracing::warn!("skipping firefox history row: {}", err),
        }
    }
    Ok(results)
}
