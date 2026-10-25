use super::{BrowserFamily, HistoryClock, HistoryKind};
use crate::{
    sqlite::{table_columns, table_exists},
    BrowserError, Result,
};
use rusqlite::Connection;

pub(super) struct Schema {
    pub(super) urls: &'static str,
    pub(super) visits: &'static str,
    pub(super) link: &'static str,
    pub(super) time: &'static str,
    pub(super) last: &'static str,
    pub(super) clock: HistoryClock,
    pub(super) url_columns: Vec<String>,
    pub(super) visit_columns: Vec<String>,
    pub(super) has_visits: bool,
}

impl Schema {
    pub(super) fn read(connection: &Connection, family: BrowserFamily) -> Result<Self> {
        let (urls, visits, link, time, last, clock) = match family {
            BrowserFamily::Chromium => (
                "urls",
                "visits",
                "url",
                "visit_time",
                "last_visit_time",
                HistoryClock::WebkitMicroseconds1601,
            ),
            BrowserFamily::Firefox => (
                "moz_places",
                "moz_historyvisits",
                "place_id",
                "visit_date",
                "last_visit_date",
                HistoryClock::UnixMicroseconds,
            ),
        };
        if !table_exists(connection, urls)? {
            return Err(BrowserError::Unsupported("missing history URL table"));
        }
        let url_columns = table_columns(connection, urls)?;
        if !url_columns.iter().any(|column| column == "id")
            || !url_columns.iter().any(|column| column == "url")
        {
            return Err(BrowserError::Unsupported("history URL columns"));
        }
        let has_visits = table_exists(connection, visits)?;
        let visit_columns = if has_visits {
            table_columns(connection, visits)?
        } else {
            Vec::new()
        };
        if has_visits
            && (!visit_columns.iter().any(|column| column == link)
                || !visit_columns.iter().any(|column| column == time))
        {
            return Err(BrowserError::Unsupported("history visit columns"));
        }
        Ok(Self {
            urls,
            visits,
            link,
            time,
            last,
            clock,
            url_columns,
            visit_columns,
            has_visits,
        })
    }

    pub(super) fn query(&self, kind: HistoryKind, inclusive: bool) -> String {
        let title = self.url_column("title");
        let count = self.url_column("visit_count");
        let operator = if inclusive { ">=" } else { ">" };
        if kind == HistoryKind::Visit {
            let id = if self.visit_columns.iter().any(|column| column == "id") {
                "v.id"
            } else {
                "NULL"
            };
            format!(
                "SELECT v.rowid,{id},v.{link},u.url,{title},{count},v.{time}
                FROM {visits} v LEFT JOIN {urls} u ON u.id=v.{link}
                WHERE v.rowid {operator} ?1 ORDER BY v.rowid LIMIT 1024",
                link = self.link,
                time = self.time,
                visits = self.visits,
                urls = self.urls
            )
        } else {
            let last = self.url_column(self.last);
            let exclude = if self.has_visits {
                format!(
                    "AND u.id NOT IN (SELECT {link} FROM {visits} WHERE {link} IS NOT NULL)",
                    link = self.link,
                    visits = self.visits
                )
            } else {
                String::new()
            };
            format!(
                "SELECT u.rowid,NULL,u.id,u.url,{title},{count},{last} FROM {urls} u
                WHERE u.rowid {operator} ?1 {exclude} ORDER BY u.rowid LIMIT 1024",
                urls = self.urls
            )
        }
    }

    fn url_column(&self, column: &str) -> String {
        if self.url_columns.iter().any(|known| known == column) {
            format!("u.{column}")
        } else {
            "NULL".into()
        }
    }
}
