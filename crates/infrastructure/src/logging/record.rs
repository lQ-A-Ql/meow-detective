use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

impl LogLevel {
    pub const ALL: [Self; 3] = [Self::Info, Self::Warning, Self::Error];

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Error => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    pub session_id: String,
    pub sequence: u64,
    pub timestamp: String,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
    pub thread_id: String,
    pub thread_name: Option<String>,
    pub truncated: bool,
}

impl LogRecord {
    pub fn field(&self, camel: &str, snake: &str) -> Option<&str> {
        self.fields
            .get(camel)
            .or_else(|| self.fields.get(snake))
            .map(String::as_str)
    }

    pub(crate) fn encode_bounded(&mut self) -> String {
        loop {
            let json = serde_json::to_string(self).expect("log record serialization");
            if json.len() <= 8 * 1024 {
                return json;
            }
            self.truncated = true;
            if self.fields.pop_last().is_none() {
                self.message
                    .truncate(self.message.floor_char_boundary(self.message.len() / 2));
            }
        }
    }
}
