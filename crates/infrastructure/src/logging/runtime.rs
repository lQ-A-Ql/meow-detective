use super::{
    file_writer::FileQueue,
    retention::{LevelBuffer, RecentLogs},
    session, ApplicationLogLayer, LogRecord, WriterHealth,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("application log I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("application log writer failed: {0}")]
    Writer(String),
    #[error("application log writer is unavailable")]
    WriterUnavailable,
    #[error("application log flush exceeded its deadline")]
    FlushTimeout,
    #[error("invalid application log query: {0}")]
    InvalidQuery(&'static str),
    #[error("application log record has expired")]
    Expired,
}

pub struct LogConfig {
    pub directory: PathBuf,
    pub records_per_level: [usize; 3],
    pub bytes_per_level: [usize; 3],
    pub queue_records: usize,
    pub queue_bytes: usize,
    pub file_bytes: u64,
    pub archives: usize,
}

impl LogConfig {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            records_per_level: [12_000, 3_000, 3_000],
            bytes_per_level: [12 * 1024 * 1024, 6 * 1024 * 1024, 6 * 1024 * 1024],
            queue_records: 4096,
            queue_bytes: 8 * 1024 * 1024,
            file_bytes: 32 * 1024 * 1024,
            archives: 7,
        }
    }
}

#[derive(Clone)]
pub struct LogRuntime(pub(crate) Arc<RuntimeState>);

pub(crate) struct RuntimeState {
    pub session_id: String,
    pub recent: Mutex<RecentLogs>,
    pub files: FileQueue,
    accepting: AtomicBool,
}

pub struct LogGuard {
    runtime: LogRuntime,
}

impl LogRuntime {
    pub fn start(config: LogConfig) -> Result<(Self, LogGuard), LogError> {
        if config.records_per_level.contains(&0)
            || config.bytes_per_level.iter().any(|bytes| *bytes < 8192)
            || config.queue_records < 4
            || config.queue_bytes < 8192
            || config.file_bytes < 8192
        {
            return Err(LogError::InvalidQuery("logging budgets are too small"));
        }
        let session = session::open(&config.directory)?;
        let session_id = session.id.clone();
        let files = FileQueue::start(session, &config)?;
        let runtime = Self(Arc::new(RuntimeState {
            session_id,
            recent: Mutex::new(RecentLogs {
                sequence: 0,
                levels: std::array::from_fn(|index| {
                    LevelBuffer::new(
                        config.records_per_level[index],
                        config.bytes_per_level[index],
                    )
                }),
            }),
            files,
            accepting: AtomicBool::new(true),
        }));
        Ok((runtime.clone(), LogGuard { runtime }))
    }

    pub fn session_id(&self) -> &str {
        &self.0.session_id
    }

    pub fn layer(&self) -> ApplicationLogLayer {
        ApplicationLogLayer(self.clone())
    }

    pub fn health(&self) -> WriterHealth {
        self.0
            .files
            .health
            .lock()
            .expect("log writer health")
            .clone()
    }

    pub fn flush(&self) -> Result<(), LogError> {
        self.0.files.flush()
    }

    pub fn reopen_writer(&self) -> Result<(), LogError> {
        self.0.files.reopen()
    }

    pub fn shutdown(&self) -> Result<(), LogError> {
        let _recent = self.0.recent.lock().expect("recent application logs");
        if !self.0.accepting.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        drop(_recent);
        self.0.files.shutdown()
    }

    pub(crate) fn capture(&self, mut record: LogRecord) {
        let mut recent = self.0.recent.lock().expect("recent application logs");
        if !self.0.accepting.load(Ordering::Acquire) {
            return;
        }
        recent.sequence = recent
            .sequence
            .checked_add(1)
            .expect("log sequence overflow");
        record.sequence = recent.sequence;
        let json = record.encode_bounded();
        let record = Arc::new(record);
        recent.levels[record.level.index()].insert(record.clone(), json.len());
        self.0.files.enqueue(record, json);
    }
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        let _ = self.runtime.shutdown();
    }
}
