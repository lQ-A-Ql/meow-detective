//! Per-family parsers for the BT panel databases.
//!
//! Every parser is table-tolerant: a missing table (version differences
//! between the legacy all-in-one `default.db` layout and the modern
//! `db/*.db` split) is skipped silently; a present table that fails to
//! read is a ParseError surfaced by the caller.
//!
//! Redaction: `password` / `salt` column values are only tested for
//! presence and never copied into attrs.

use serde_json::{Map, Value};

use crate::db::{integer, sensitive_present, text, PanelDb};
use crate::payload::{new_attrs, put_opt, Payload};
use crate::time::{to_local_iso, TIMEZONE_WARNING};

pub const ACCOUNTS: &str = "accounts";
pub const SITES: &str = "sites";
pub const DATABASES: &str = "databases";
pub const FTPS: &str = "ftps";
pub const FIREWALL: &str = "firewall";
pub const CRONTAB: &str = "crontab";
pub const LOGS: &str = "logs";

/// Bound on emitted log rows so a pathological `log.db` cannot blow up the
/// payload; the host reads the whole payload into memory.
const MAX_LOG_ROWS: usize = 20_000;

pub fn run(route: &str, db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    match route {
        ACCOUNTS => accounts(db, payload),
        SITES => sites(db, payload),
        DATABASES => databases(db, payload),
        FTPS => ftps(db, payload),
        FIREWALL => firewall(db, payload),
        CRONTAB => crontab(db, payload),
        LOGS => logs(db, payload),
        _ => Ok(()),
    }
}

/// Convert a panel-local timestamp and remember to flag the UTC assumption
/// once per database. Inserts nothing when the raw value is absent.
fn stamp(
    payload: &mut Payload,
    attrs_out: &mut Map<String, Value>,
    key: &str,
    raw: Option<&str>,
    warned: &mut bool,
) {
    let Some(raw) = raw else { return };
    if let Some(utc) = to_local_iso(raw) {
        attrs_out.insert(key.to_string(), Value::String(utc));
        if !*warned {
            payload.warn(TIMEZONE_WARNING);
            *warned = true;
        }
    }
}

/// `users` → BtPanelAccount. Password hashes and salts are redacted to a
/// presence flag plus the recognized algorithm shape.
fn accounts(db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    if !db.table_exists("users")? {
        return Ok(());
    }
    let mut warned = false;
    for row in db.rows("users")? {
        let mut attrs = new_attrs();
        put_opt(&mut attrs, "userId", row.get("id"));
        put_opt(&mut attrs, "username", row.get("username"));
        put_opt(&mut attrs, "loginIp", row.get("login_ip"));
        put_opt(&mut attrs, "phone", row.get("phone"));
        put_opt(&mut attrs, "email", row.get("email"));
        let salted = row.contains_key("salt");
        // Classify the algorithm shape without exposing secret material:
        // only the fixed "BT-0x:" format marker prefix is inspected.
        let algorithm = match text(&row, "password") {
            Some(password) if password.starts_with("BT-0x:") => {
                "BT-0x proprietary (panel 9.x format)"
            }
            _ if salted => "md5(md5(md5(password)+'_bt.cn')+salt)",
            _ => "md5(md5(password)+'_bt.cn') (legacy, no salt column)",
        };
        attrs.insert(
            "hasPasswordHash".to_string(),
            Value::Bool(sensitive_present(&row, "password")),
        );
        attrs.insert(
            "passwordAlgorithm".to_string(),
            Value::String(algorithm.to_string()),
        );
        stamp(
            payload,
            &mut attrs,
            "loginTimeLocal",
            text(&row, "login_time"),
            &mut warned,
        );
        let username = text(&row, "username").unwrap_or("<unknown>").to_string();
        let summary = match text(&row, "login_ip") {
            Some(ip) => format!("login_ip={ip}"),
            None => String::new(),
        };
        payload.artifact("BtPanelAccount", username, summary, attrs);
    }
    Ok(())
}

/// `sites` + `domain` → BtPanelSite. Domains join onto sites via `pid`;
/// orphaned domain rows are emitted as standalone artifacts.
fn sites(db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    let has_sites = db.table_exists("sites")?;
    let has_domains = db.table_exists("domain")?;
    if !has_sites && !has_domains {
        return Ok(());
    }
    let domains = if has_domains {
        db.rows("domain")?
    } else {
        Vec::new()
    };
    let mut claimed = vec![false; domains.len()];
    let mut warned = false;
    if has_sites {
        for row in db.rows("sites")? {
            let site_id = integer(&row, "id");
            let mut attrs = new_attrs();
            put_opt(&mut attrs, "siteId", row.get("id"));
            put_opt(&mut attrs, "siteName", row.get("name"));
            put_opt(&mut attrs, "path", row.get("path"));
            let status = text(&row, "status").unwrap_or_default();
            if !status.is_empty() {
                attrs.insert("status".to_string(), Value::String(status.to_string()));
                let decoded = match status {
                    "1" => "running",
                    "0" => "stopped",
                    _ => "unknown",
                };
                attrs.insert("statusText".to_string(), Value::String(decoded.to_string()));
            }
            put_opt(&mut attrs, "indexPages", row.get("index"));
            put_opt(&mut attrs, "remark", row.get("ps"));
            stamp(
                payload,
                &mut attrs,
                "addtimeLocal",
                text(&row, "addtime"),
                &mut warned,
            );
            let mut names = Vec::new();
            for (index, domain) in domains.iter().enumerate() {
                if site_id.is_some() && integer(domain, "pid") == site_id {
                    claimed[index] = true;
                    if let Some(name) = text(domain, "name") {
                        let port = integer(domain, "port").unwrap_or(80);
                        names.push(Value::String(format!("{name}:{port}")));
                    }
                }
            }
            if !names.is_empty() {
                attrs.insert("domains".to_string(), Value::Array(names));
            }
            let name = text(&row, "name").unwrap_or("<unnamed>").to_string();
            let summary = text(&row, "path").unwrap_or_default().to_string();
            payload.artifact("BtPanelSite", name, summary, attrs);
        }
    }
    for (index, domain) in domains.iter().enumerate() {
        if claimed[index] {
            continue;
        }
        let mut attrs = new_attrs();
        put_opt(&mut attrs, "domainId", domain.get("id"));
        put_opt(&mut attrs, "port", domain.get("port"));
        attrs.insert("orphan".to_string(), Value::Bool(true));
        stamp(
            payload,
            &mut attrs,
            "addtimeLocal",
            text(domain, "addtime"),
            &mut warned,
        );
        let name = text(domain, "name").unwrap_or("<unnamed>").to_string();
        payload.artifact(
            "BtPanelSite",
            name.clone(),
            format!("orphan domain {name}"),
            attrs,
        );
    }
    Ok(())
}

/// `databases` → BtPanelDatabase. The per-database password is redacted.
fn databases(db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    if !db.table_exists("databases")? {
        return Ok(());
    }
    let mut warned = false;
    for row in db.rows("databases")? {
        let mut attrs = new_attrs();
        put_opt(&mut attrs, "databaseId", row.get("id"));
        put_opt(&mut attrs, "databaseName", row.get("name"));
        put_opt(&mut attrs, "siteId", row.get("pid"));
        put_opt(&mut attrs, "username", row.get("username"));
        put_opt(
            &mut attrs,
            "dbType",
            row.get("type").or_else(|| row.get("db_type")),
        );
        put_opt(&mut attrs, "accept", row.get("accept"));
        put_opt(&mut attrs, "remark", row.get("ps"));
        attrs.insert(
            "hasPassword".to_string(),
            Value::Bool(sensitive_present(&row, "password")),
        );
        stamp(
            payload,
            &mut attrs,
            "addtimeLocal",
            text(&row, "addtime"),
            &mut warned,
        );
        let name = text(&row, "name").unwrap_or("<unnamed>").to_string();
        let summary = text(&row, "username")
            .map(|user| format!("user={user}"))
            .unwrap_or_default();
        payload.artifact("BtPanelDatabase", name, summary, attrs);
    }
    Ok(())
}

/// `ftps` → BtPanelFtp. FTP account passwords are redacted.
fn ftps(db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    if !db.table_exists("ftps")? {
        return Ok(());
    }
    let mut warned = false;
    for row in db.rows("ftps")? {
        let mut attrs = new_attrs();
        put_opt(&mut attrs, "ftpId", row.get("id"));
        put_opt(&mut attrs, "username", row.get("name"));
        put_opt(&mut attrs, "siteId", row.get("pid"));
        put_opt(&mut attrs, "path", row.get("path"));
        put_opt(&mut attrs, "status", row.get("status"));
        put_opt(&mut attrs, "remark", row.get("ps"));
        attrs.insert(
            "hasPassword".to_string(),
            Value::Bool(sensitive_present(&row, "password")),
        );
        stamp(
            payload,
            &mut attrs,
            "addtimeLocal",
            text(&row, "addtime"),
            &mut warned,
        );
        let name = text(&row, "name").unwrap_or("<unnamed>").to_string();
        let summary = text(&row, "path").unwrap_or_default().to_string();
        payload.artifact("BtPanelFtp", name, summary, attrs);
    }
    Ok(())
}

/// Firewall rules → BtPanelFirewall. Covers the legacy `firewall` port
/// table, the older `firewall_acceptip` IP table, and the modern
/// `firewall_new` / `firewall_ip` tables (7.7+/9.x).
fn firewall(db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    let mut warned = false;
    if db.table_exists("firewall")? {
        for row in db.rows("firewall")? {
            let mut attrs = new_attrs();
            put_opt(&mut attrs, "ruleId", row.get("id"));
            put_opt(&mut attrs, "port", row.get("port"));
            put_opt(&mut attrs, "protocol", row.get("protocol"));
            put_opt(&mut attrs, "sourceIp", row.get("address"));
            put_opt(&mut attrs, "remark", row.get("ps"));
            // Panel port rules are accept-by-default allowlist entries.
            attrs.insert("policy".to_string(), Value::String("accept".to_string()));
            stamp(
                payload,
                &mut attrs,
                "addtimeLocal",
                text(&row, "addtime"),
                &mut warned,
            );
            let title = text(&row, "port")
                .map(|port| format!("port {port}"))
                .unwrap_or_else(|| "<rule>".to_string());
            let summary = text(&row, "ps").unwrap_or_default().to_string();
            payload.artifact("BtPanelFirewall", title, summary, attrs);
        }
    }
    if db.table_exists("firewall_acceptip")? {
        for row in db.rows("firewall_acceptip")? {
            let mut attrs = new_attrs();
            put_opt(&mut attrs, "ruleId", row.get("id"));
            put_opt(&mut attrs, "sourceIp", row.get("address"));
            put_opt(&mut attrs, "port", row.get("port"));
            put_opt(&mut attrs, "remark", row.get("ps"));
            let policy = match text(&row, "types") {
                Some("drop") => "drop",
                _ => "accept",
            };
            attrs.insert("policy".to_string(), Value::String(policy.to_string()));
            stamp(
                payload,
                &mut attrs,
                "addtimeLocal",
                text(&row, "addtime"),
                &mut warned,
            );
            let title = text(&row, "address")
                .map(|ip| format!("{policy} {ip}"))
                .unwrap_or_else(|| "<ip-rule>".to_string());
            payload.artifact("BtPanelFirewall", title, String::new(), attrs);
        }
    }
    // Modern panels (7.7+/9.x) keep the richer rule set in firewall_new
    // (protocol/ports/types/address) and per-IP rules in firewall_ip.
    if db.table_exists("firewall_new")? {
        for row in db.rows("firewall_new")? {
            let mut attrs = new_attrs();
            put_opt(&mut attrs, "ruleId", row.get("id"));
            put_opt(&mut attrs, "protocol", row.get("protocol"));
            put_opt(&mut attrs, "ports", row.get("ports"));
            put_opt(&mut attrs, "sourceIp", row.get("address"));
            put_opt(&mut attrs, "chain", row.get("chain"));
            put_opt(&mut attrs, "remark", row.get("brief"));
            let policy = match text(&row, "types") {
                Some("drop") => "drop",
                _ => "accept",
            };
            attrs.insert("policy".to_string(), Value::String(policy.to_string()));
            stamp(
                payload,
                &mut attrs,
                "addtimeLocal",
                text(&row, "addtime"),
                &mut warned,
            );
            let title = format!(
                "{policy} {}/{}",
                text(&row, "protocol").unwrap_or("tcp"),
                text(&row, "ports").unwrap_or("?")
            );
            let summary = text(&row, "brief").unwrap_or_default().to_string();
            payload.artifact("BtPanelFirewall", title, summary, attrs);
        }
    }
    if db.table_exists("firewall_ip")? {
        for row in db.rows("firewall_ip")? {
            let mut attrs = new_attrs();
            put_opt(&mut attrs, "ruleId", row.get("id"));
            put_opt(&mut attrs, "sourceIp", row.get("address"));
            put_opt(&mut attrs, "chain", row.get("chain"));
            put_opt(&mut attrs, "remark", row.get("brief"));
            let policy = match text(&row, "types") {
                Some("drop") => "drop",
                _ => "accept",
            };
            attrs.insert("policy".to_string(), Value::String(policy.to_string()));
            stamp(
                payload,
                &mut attrs,
                "addtimeLocal",
                text(&row, "addtime"),
                &mut warned,
            );
            let title = text(&row, "address")
                .map(|ip| format!("{policy} {ip}"))
                .unwrap_or_else(|| "<ip-rule>".to_string());
            payload.artifact("BtPanelFirewall", title, String::new(), attrs);
        }
    }
    Ok(())
}

/// `crontab` → BtPanelTask (panel-managed scheduled tasks).
fn crontab(db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    if !db.table_exists("crontab")? {
        return Ok(());
    }
    let mut warned = false;
    for row in db.rows("crontab")? {
        let mut attrs = new_attrs();
        put_opt(&mut attrs, "taskId", row.get("id"));
        put_opt(&mut attrs, "taskName", row.get("name"));
        put_opt(&mut attrs, "cycleType", row.get("type"));
        put_opt(&mut attrs, "where1", row.get("where1"));
        put_opt(&mut attrs, "hour", row.get("where_hour"));
        put_opt(&mut attrs, "minute", row.get("where_minute"));
        put_opt(&mut attrs, "week", row.get("week"));
        // Newer schemas carry the command body in sBody; the legacy `echo`
        // column is the generated shell-script basename.
        put_opt(
            &mut attrs,
            "command",
            row.get("sBody").or_else(|| row.get("sName")),
        );
        put_opt(&mut attrs, "scriptEcho", row.get("echo"));
        put_opt(&mut attrs, "backupTo", row.get("backupTo"));
        put_opt(&mut attrs, "status", row.get("status"));
        stamp(
            payload,
            &mut attrs,
            "addtimeLocal",
            text(&row, "addtime"),
            &mut warned,
        );
        let name = text(&row, "name").unwrap_or("<unnamed>").to_string();
        let summary = text(&row, "type").unwrap_or_default().to_string();
        payload.artifact("BtPanelTask", name, summary, attrs);
    }
    Ok(())
}

/// `logs` → BtPanelLog artifacts plus one timeline event per row.
fn logs(db: &PanelDb, payload: &mut Payload) -> Result<(), String> {
    if !db.table_exists("logs")? {
        return Ok(());
    }
    let rows = db.rows("logs")?;
    if rows.len() > MAX_LOG_ROWS {
        payload.warn(format!(
            "logs table truncated: {} rows, emitting first {MAX_LOG_ROWS}",
            rows.len()
        ));
    }
    let mut warned = false;
    for row in rows.iter().take(MAX_LOG_ROWS) {
        let mut attrs = new_attrs();
        put_opt(&mut attrs, "logId", row.get("id"));
        put_opt(&mut attrs, "logType", row.get("type"));
        put_opt(&mut attrs, "content", row.get("log"));
        put_opt(&mut attrs, "username", row.get("username"));
        put_opt(&mut attrs, "uid", row.get("uid"));
        stamp(
            payload,
            &mut attrs,
            "addtimeLocal",
            text(row, "addtime"),
            &mut warned,
        );
        let log_type = text(row, "type").unwrap_or("unknown");
        let content = text(row, "log").unwrap_or_default();
        let title = format!("{log_type}: {}", truncate(content, 60));
        payload.artifact("BtPanelLog", title, String::new(), attrs);
        if let Some(utc) = text(row, "addtime").and_then(to_local_iso) {
            let mut event_attrs = new_attrs();
            put_opt(&mut event_attrs, "logType", row.get("type"));
            put_opt(&mut event_attrs, "username", row.get("username"));
            payload.timeline_event(
                utc,
                "BT_PANEL_OPERATION",
                format!("{log_type}: {content}"),
                event_attrs,
            );
        }
    }
    Ok(())
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut cut: String = value.chars().take(max).collect();
    cut.push('…');
    cut
}
