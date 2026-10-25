mod schema;
mod visit;
use crate::BrowserError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
pub use visit::visit_history;

#[derive(Debug, Clone, Copy)]
pub enum BrowserFamily {
    Chromium,
    Firefox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryKind {
    Visit,
    UrlSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryClock {
    WebkitMicroseconds1601,
    UnixMicroseconds,
}

impl HistoryClock {
    pub fn name(self) -> &'static str {
        match self {
            Self::WebkitMicroseconds1601 => "webkitMicroseconds1601",
            Self::UnixMicroseconds => "unixMicroseconds",
        }
    }
    pub fn datetime(self, raw: i64) -> Option<DateTime<Utc>> {
        match self {
            Self::WebkitMicroseconds1601 => crate::chromium::time::webkit_time_to_dt(raw),
            Self::UnixMicroseconds => crate::firefox::unix_microseconds_to_dt(raw),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub row_id: i64,
    pub visit_id: Option<i64>,
    pub url_id: Option<i64>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub visit_count: Option<i64>,
    pub raw_time: Option<i64>,
    pub clock: HistoryClock,
    pub kind: HistoryKind,
}

#[derive(Debug, thiserror::Error)]
pub enum VisitError<E> {
    #[error("{0}")]
    Parser(BrowserError),
    #[error("{0}")]
    Sink(E),
}

impl<E> From<BrowserError> for VisitError<E> {
    fn from(error: BrowserError) -> Self {
        Self::Parser(error)
    }
}

pub(crate) fn collect_history(
    bytes: &[u8],
    family: BrowserFamily,
    browser: &str,
    profile: Option<&str>,
) -> crate::Result<Vec<crate::BrowserVisit>> {
    let connection = crate::sqlite::open_sqlite_from_bytes(bytes)?;
    let mut records = Vec::new();
    let result = visit_history(
        &connection,
        family,
        &std::sync::atomic::AtomicBool::new(false),
        |row| {
            let time = if row.kind == HistoryKind::Visit {
                row.raw_time.and_then(|raw| row.clock.datetime(raw))
            } else {
                None
            };
            records.push(crate::BrowserVisit {
                url: row.url.unwrap_or_default(),
                title: row.title,
                visit_time: time,
                visit_count: row.visit_count.unwrap_or(0),
                browser: browser.into(),
                profile: profile.map(str::to_string),
                row_id: Some(row.row_id),
                visit_id: row.visit_id,
                url_id: row.url_id,
                raw_time: row.raw_time,
                time_basis: Some(row.clock),
                record_kind: Some(row.kind),
            });
            Ok::<(), BrowserError>(())
        },
    );
    match result {
        Ok(()) => Ok(records),
        Err(VisitError::Parser(error) | VisitError::Sink(error)) => Err(error),
    }
}
