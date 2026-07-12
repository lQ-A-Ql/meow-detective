use super::sqlite::open_sqlite_from_bytes;
use super::time::webkit_time_to_dt;
use super::types::BrowserCookie;
use rusqlite::params;

/// Parse a Chromium `Cookies` SQLite database.
///
/// The `value` column may contain DPAPI-encrypted blobs in modern Chrome/Edge;
/// `value_preview` remains metadata-only for values that appear encrypted.
pub fn parse_chrome_cookies(
    data: &[u8],
    _browser: &str,
    _profile: Option<&str>,
) -> Result<Vec<BrowserCookie>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn
        .prepare(
            "SELECT host_key, name, value, encrypted_value, expires_utc,
                    is_secure, is_httponly, samesite
             FROM cookies
             ORDER BY host_key",
        )
        .map_err(|e| format!("prepare cookies query: {}", e))?;

    let rows = stmt
        .query_map(params![], |row| {
            let domain: String = row.get(0)?;
            let name: String = row.get(1)?;
            let raw_value: Option<String> = row.get(2).ok();
            let encrypted_value: Option<Vec<u8>> = row.get(3).ok();
            let value_preview =
                cookie_value_preview(raw_value.as_deref(), encrypted_value.as_deref());
            let expires_utc_raw: i64 = row.get(4).unwrap_or(0);
            let secure: bool = row.get(5).unwrap_or(false);
            let http_only: bool = row.get(6).unwrap_or(false);
            let same_site: Option<i64> = row.get(7).ok();

            Ok(BrowserCookie {
                domain,
                name,
                value_preview,
                expiry: webkit_time_to_dt(expires_utc_raw),
                secure,
                http_only,
                same_site,
            })
        })
        .map_err(|e| format!("query cookies: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(cookie) => results.push(cookie),
            Err(e) => tracing::warn!("skipping cookie row: {}", e),
        }
    }

    Ok(results)
}

fn cookie_value_preview(raw_value: Option<&str>, encrypted_value: Option<&[u8]>) -> Option<String> {
    match raw_value {
        Some("") => encrypted_value.map(|bytes| format!("[encrypted {} bytes]", bytes.len())),
        Some(value) if is_likely_encrypted(value) => {
            Some(format!("[encrypted {} bytes]", value.len()))
        }
        Some(value) => Some(value.chars().take(128).collect()),
        None => encrypted_value.map(|bytes| format!("[encrypted {} bytes]", bytes.len())),
    }
}

pub(super) fn is_likely_encrypted(value: &str) -> bool {
    if value.len() < 8 {
        return false;
    }

    let bytes = value.as_bytes();
    let non_printable = bytes
        .iter()
        .filter(|&&byte| !(0x20..=0x7e).contains(&byte))
        .count();
    (non_printable as f64) > (bytes.len() as f64 * 0.3)
}
