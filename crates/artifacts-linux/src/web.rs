use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod config;
mod findings;
mod logs;
mod util;

pub use config::{parse_apache_config, parse_nginx_config};
pub use findings::{detect_web_findings, detect_web_shell};
pub use logs::{parse_web_access_log, parse_web_error_log};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebSite {
    pub server_kind: String,
    pub site_name: String,
    pub hostnames: Vec<String>,
    pub listen: Vec<String>,
    pub document_roots: Vec<String>,
    pub access_logs: Vec<String>,
    pub error_logs: Vec<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebAccessLogEntry {
    pub client_ip: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub method: String,
    pub uri: String,
    pub protocol: String,
    pub status: u16,
    pub response_bytes: Option<u64>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebErrorLogEntry {
    pub timestamp: Option<String>,
    pub severity: Option<String>,
    pub message: String,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebFinding {
    pub finding_kind: String,
    pub severity: String,
    pub confidence: f32,
    pub evidence: String,
    pub client_ip: Option<String>,
    pub uri: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub line_number: u64,
}

#[cfg(test)]
#[path = "../tests/unit/web.rs"]
mod tests;
