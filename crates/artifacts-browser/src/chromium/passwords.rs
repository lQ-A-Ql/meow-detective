use super::sqlite::open_sqlite_from_bytes;
use super::time::webkit_time_to_dt;
use super::types::{BrowserDecryptionStatus, BrowserPassword};
use crate::BrowserSecretDecryptor;
use rusqlite::params;

/// Parse a Chromium `Login Data` SQLite database.
///
/// The `password_value` column is DPAPI-encrypted and is surfaced only as an
/// `[encrypted N bytes]` preview.
pub fn parse_chrome_passwords(
    data: &[u8],
    browser: &str,
    profile: Option<&str>,
) -> crate::Result<Vec<BrowserPassword>> {
    parse_chrome_passwords_with_decryptor(data, browser, profile, None)
}

/// Parse Chromium Login Data and optionally decrypt password values.
pub fn parse_chrome_passwords_with_decryptor(
    data: &[u8],
    browser: &str,
    profile: Option<&str>,
    decryptor: Option<&dyn BrowserSecretDecryptor>,
) -> crate::Result<Vec<BrowserPassword>> {
    let conn = open_sqlite_from_bytes(data)?;
    parse_chrome_passwords_from_connection(&conn, browser, profile, decryptor)
}

pub fn parse_chrome_passwords_from_connection(
    conn: &rusqlite::Connection,
    browser: &str,
    profile: Option<&str>,
    decryptor: Option<&dyn BrowserSecretDecryptor>,
) -> crate::Result<Vec<BrowserPassword>> {
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
            let (password_preview, decryption_status, decryption_detail) =
                password_preview(password_value.as_deref(), decryptor);

            Ok(BrowserPassword {
                url,
                username,
                password_preview,
                created_at: webkit_time_to_dt(date_created),
                times_used: times_used.max(0),
                browser: browser.to_string(),
                profile: profile.map(|s| s.to_string()),
                decryption_status,
                decryption_detail,
            })
        })
        .map_err(|e| format!("query passwords: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    Ok(results)
}

fn password_preview(
    encrypted: Option<&[u8]>,
    decryptor: Option<&dyn BrowserSecretDecryptor>,
) -> (Option<String>, BrowserDecryptionStatus, Option<String>) {
    let Some(bytes) = encrypted.filter(|bytes| !bytes.is_empty()) else {
        return (None, BrowserDecryptionStatus::Plaintext, None);
    };
    let Some(decryptor) = decryptor else {
        return (
            Some(format!("[encrypted {} bytes]", bytes.len())),
            BrowserDecryptionStatus::Encrypted,
            Some("offline DPAPI context is unavailable".to_string()),
        );
    };
    let (status, preview, detail) = decryptor.decrypt_value(bytes, None);
    (preview, status, detail)
}
