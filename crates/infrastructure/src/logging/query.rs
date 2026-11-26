use super::{LogError, LogLevel, LogRecord, LogRuntime, WriterHealth};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDirection {
    Latest,
    Forward,
    Backward,
}

#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub text: String,
    pub target: String,
    pub case_id: String,
    pub data_source_id: String,
    pub job_id: String,
}

impl LogFilter {
    fn matches(&self, record: &LogRecord) -> bool {
        (self.text.is_empty()
            || record
                .message
                .to_lowercase()
                .contains(&self.text.to_lowercase())
            || record
                .fields
                .values()
                .any(|value| value.to_lowercase().contains(&self.text.to_lowercase())))
            && (self.target.is_empty() || record.target.contains(&self.target))
            && matches_field(record, "caseId", "case_id", &self.case_id)
            && matches_field(
                record,
                "dataSourceId",
                "data_source_id",
                &self.data_source_id,
            )
            && matches_field(record, "jobId", "job_id", &self.job_id)
    }
}

fn matches_field(record: &LogRecord, camel: &str, snake: &str, value: &str) -> bool {
    value.is_empty() || record.field(camel, snake) == Some(value)
}

pub struct LogQuery {
    pub direction: LogDirection,
    pub positions: [Option<u64>; 3],
    pub filter: LogFilter,
    pub limit_per_level: usize,
}

pub struct LogPage {
    pub level: LogLevel,
    pub records: Vec<Arc<LogRecord>>,
    pub forward: u64,
    pub backward: u64,
    pub has_more: bool,
    pub gap: bool,
    pub emitted: u64,
    pub evicted: u64,
    pub truncated: u64,
}

pub struct LogSnapshot {
    pub session_id: String,
    pub upper_sequence: u64,
    pub pages: Vec<LogPage>,
    pub writer: WriterHealth,
}

impl LogRuntime {
    pub fn read(&self, query: LogQuery) -> Result<LogSnapshot, LogError> {
        if query.limit_per_level == 0 || query.limit_per_level > 100 {
            return Err(LogError::InvalidQuery("per-level limit must be in 1..=100"));
        }
        let recent = self.0.recent.lock().expect("recent application logs");
        let mut pages = Vec::with_capacity(3);
        for level in LogLevel::ALL {
            let buffer = &recent.levels[level.index()];
            let position = query.positions[level.index()].unwrap_or(recent.sequence);
            let forward = query.direction == LogDirection::Forward;
            let start = buffer
                .records
                .partition_point(|entry| entry.record.sequence <= position);
            let candidates: Box<dyn Iterator<Item = _>> = if forward {
                Box::new(buffer.records.range(start..))
            } else {
                let end = if query.direction == LogDirection::Latest {
                    buffer.records.len()
                } else {
                    buffer
                        .records
                        .partition_point(|entry| entry.record.sequence < position)
                };
                Box::new(buffer.records.range(..end).rev())
            };
            let mut records = Vec::new();
            let mut bytes = 0;
            let mut scanned = 0;
            let mut last = position;
            let mut more = false;
            for entry in candidates {
                if scanned == 1365
                    || records.len() == query.limit_per_level
                    || bytes + entry.bytes > 32 * 1024
                {
                    more = true;
                    break;
                }
                scanned += 1;
                last = entry.record.sequence;
                if query.filter.matches(&entry.record) {
                    bytes += entry.bytes;
                    records.push(entry.record.clone());
                }
            }
            records.sort_by_key(|record| record.sequence);
            let gap = forward && position < buffer.last_evicted_sequence;
            pages.push(LogPage {
                level,
                records,
                forward: if forward {
                    if more {
                        last
                    } else {
                        recent.sequence
                    }
                } else {
                    recent.sequence
                },
                backward: if scanned > 0 { last } else { position },
                has_more: more,
                gap,
                emitted: buffer.emitted,
                evicted: buffer.evicted,
                truncated: buffer.truncated,
            });
        }
        Ok(LogSnapshot {
            session_id: self.session_id().to_owned(),
            upper_sequence: recent.sequence,
            pages,
            writer: self.health(),
        })
    }

    pub fn record(&self, sequence: u64) -> Result<Arc<LogRecord>, LogError> {
        let recent = self.0.recent.lock().expect("recent application logs");
        recent
            .levels
            .iter()
            .find_map(|buffer| {
                let index = buffer
                    .records
                    .partition_point(|entry| entry.record.sequence < sequence);
                buffer
                    .records
                    .get(index)
                    .filter(|entry| entry.record.sequence == sequence)
                    .map(|entry| entry.record.clone())
            })
            .ok_or(LogError::Expired)
    }
}
