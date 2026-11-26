use super::LogRecord;
use std::{collections::VecDeque, sync::Arc};

pub(crate) struct RetainedRecord {
    pub record: Arc<LogRecord>,
    pub bytes: usize,
}

pub(crate) struct LevelBuffer {
    pub records: VecDeque<RetainedRecord>,
    pub emitted: u64,
    pub evicted: u64,
    pub last_evicted_sequence: u64,
    pub truncated: u64,
    bytes: usize,
    max_records: usize,
    max_bytes: usize,
}

impl LevelBuffer {
    pub fn new(max_records: usize, max_bytes: usize) -> Self {
        Self {
            records: VecDeque::new(),
            emitted: 0,
            evicted: 0,
            last_evicted_sequence: 0,
            truncated: 0,
            bytes: 0,
            max_records,
            max_bytes,
        }
    }

    pub fn insert(&mut self, record: Arc<LogRecord>, bytes: usize) {
        self.emitted += 1;
        self.truncated += u64::from(record.truncated);
        self.bytes += bytes;
        self.records.push_back(RetainedRecord { record, bytes });
        while self.records.len() > self.max_records || self.bytes > self.max_bytes {
            let oldest = self.records.pop_front().expect("buffer exceeds its budget");
            self.bytes -= oldest.bytes;
            self.evicted += 1;
            self.last_evicted_sequence = oldest.record.sequence;
        }
    }
}

pub(crate) struct RecentLogs {
    pub sequence: u64,
    pub levels: [LevelBuffer; 3],
}
