use super::time::unix_millis_to_dt;
use crate::browser::chromium::BrowserPassword;
use serde_json::Value;

pub fn parse_firefox_passwords(data: &[u8]) -> Result<Vec<BrowserPassword>, String> {
    let text =
        std::str::from_utf8(data).map_err(|e| format!("logins.json is not valid UTF-8: {}", e))?;
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("logins.json parse error: {}", e))?;
    let logins = match root.get("logins") {
        Some(Value::Array(logins)) => logins,
        _ => return Ok(Vec::new()),
    };

    Ok(logins
        .iter()
        .map(|entry| {
            let password = entry
                .get("encryptedPassword")
                .and_then(Value::as_str)
                .unwrap_or("");
            BrowserPassword {
                url: string_field(entry, "hostname"),
                username: string_field(entry, "encryptedUsername"),
                password_preview: if password.is_empty() {
                    None
                } else {
                    Some(format!("[encrypted {} bytes]", password.len()))
                },
                created_at: entry
                    .get("timeCreated")
                    .and_then(Value::as_i64)
                    .and_then(unix_millis_to_dt),
                times_used: entry
                    .get("timesUsed")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .max(0),
                browser: "Firefox".to_string(),
                profile: None,
            }
        })
        .collect())
}

fn string_field(entry: &Value, name: &str) -> String {
    entry
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
