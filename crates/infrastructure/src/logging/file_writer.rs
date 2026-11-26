use super::{session::LogSession, LogConfig, LogError, LogRecord};
use crossbeam_channel::{Receiver, Sender};
use flexi_logger::{
    writers::{FileLogWriter, LogWriter},
    Cleanup, Criterion, DeferredNow, FileSpec, Naming, WriteMode,
};
use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug, Clone, Default)]
pub struct WriterHealth {
    pub failure: Option<String>,
    pub dropped: [u64; 3],
    pub failed_records: u64,
    pub queued_records: usize,
    pub queued_bytes: usize,
    pub written_sequence: u64,
}

struct FileRecord {
    record: Arc<LogRecord>,
    json: String,
}

enum Control {
    Flush(Sender<Result<(), String>>),
    Reopen(Sender<Result<(), String>>),
    Shutdown(Sender<Result<(), String>>),
}

pub(crate) struct FileQueue {
    records: Sender<FileRecord>,
    control: Sender<Control>,
    pub health: Arc<Mutex<WriterHealth>>,
    capacity: usize,
    budget: usize,
}

impl FileQueue {
    pub fn start(session: LogSession, config: &LogConfig) -> Result<Self, LogError> {
        let writer = DiskWriter::open(
            session.directory.clone(),
            config.file_bytes,
            config.archives,
        )?;
        let (records, input) = crossbeam_channel::bounded(config.queue_records);
        let (control, commands) = crossbeam_channel::bounded(8);
        let health = Arc::new(Mutex::new(WriterHealth::default()));
        let worker_health = health.clone();
        std::thread::Builder::new()
            .name("application-log-writer".into())
            .spawn(move || {
                let _lease = session.lease;
                run_worker(writer, input, commands, worker_health);
            })?;
        Ok(Self {
            records,
            control,
            health,
            capacity: config.queue_records,
            budget: config.queue_bytes,
        })
    }

    pub fn enqueue(&self, record: Arc<LogRecord>, json: String) {
        let index = record.level.index();
        let watermark = [75, 90, 100][index];
        let mut health = self.health.lock().expect("log writer health");
        let bytes = json.len() * 2;
        if health.queued_records + 1 > self.capacity * watermark / 100
            || health.queued_bytes + bytes > self.budget * watermark / 100
        {
            health.dropped[index] += 1;
            return;
        }
        health.queued_records += 1;
        health.queued_bytes += bytes;
        if self.records.try_send(FileRecord { record, json }).is_err() {
            health.queued_records -= 1;
            health.queued_bytes -= bytes;
            health.dropped[index] += 1;
        }
    }

    pub fn flush(&self) -> Result<(), LogError> {
        self.request(Control::Flush)
    }

    pub fn reopen(&self) -> Result<(), LogError> {
        self.request(Control::Reopen)
    }

    pub fn shutdown(&self) -> Result<(), LogError> {
        self.request(Control::Shutdown)
    }

    fn request(
        &self,
        make: impl FnOnce(Sender<Result<(), String>>) -> Control,
    ) -> Result<(), LogError> {
        let (reply, response) = crossbeam_channel::bounded(1);
        self.control
            .send_timeout(make(reply), Duration::from_millis(200))
            .map_err(|_| LogError::WriterUnavailable)?;
        response
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| LogError::FlushTimeout)?
            .map_err(LogError::Writer)
    }
}

fn run_worker(
    mut writer: DiskWriter,
    input: Receiver<FileRecord>,
    control: Receiver<Control>,
    health: Arc<Mutex<WriterHealth>>,
) {
    loop {
        crossbeam_channel::select_biased! {
            recv(control) -> command => {
                let Ok(command) = command else { break };
                let shutdown = matches!(command, Control::Shutdown(_));
                for _ in 0..input.len() {
                    if let Ok(record) = input.try_recv() {
                        write_one(&mut writer, record, &health);
                    }
                }
                let (reply, result) = match command {
                    Control::Reopen(reply) => (reply, writer.reopen()),
                    Control::Flush(reply) | Control::Shutdown(reply) => (reply, writer.flush()),
                };
                health.lock().expect("log writer health").failure = result.as_ref().err().cloned();
                let _ = reply.send(result);
                if shutdown { break; }
            }
            recv(input) -> record => {
                let Ok(record) = record else { break };
                write_one(&mut writer, record, &health);
            }
            default(Duration::from_secs(1)) => {
                if let Err(error) = writer.flush() {
                    health.lock().expect("log writer health").failure = Some(error);
                }
            }
        }
    }
}

fn write_one(writer: &mut DiskWriter, input: FileRecord, health: &Mutex<WriterHealth>) {
    {
        let mut health = health.lock().expect("log writer health");
        health.queued_records -= 1;
        health.queued_bytes -= input.json.len() * 2;
    }
    let result = writer.write(&input);
    let mut health = health.lock().expect("log writer health");
    match result {
        Ok(()) => health.written_sequence = input.record.sequence,
        Err(error) => {
            health.failure = Some(error);
            health.failed_records += 1;
        }
    }
}

struct DiskWriter {
    writer: FileLogWriter,
    directory: PathBuf,
    size: u64,
    limit: u64,
    archives: usize,
    failure: Option<String>,
}

impl DiskWriter {
    fn open(directory: PathBuf, limit: u64, archives: usize) -> Result<Self, LogError> {
        let current = directory.join("runtime_rCURRENT.jsonl");
        let size = std::fs::File::options()
            .create(true)
            .append(true)
            .open(&current)?
            .metadata()?
            .len();
        let writer = FileLogWriter::builder(
            FileSpec::default()
                .directory(&directory)
                .basename("runtime")
                .suppress_timestamp()
                .suffix("jsonl"),
        )
        .format(json_format)
        .write_mode(WriteMode::Direct)
        .cleanup_in_background_thread(false)
        .rotate(
            Criterion::Size(limit),
            Naming::Numbers,
            Cleanup::KeepLogFiles(archives),
        )
        .append()
        .try_build()
        .map_err(|error| LogError::Writer(error.to_string()))?;
        Ok(Self {
            writer,
            directory,
            size,
            limit,
            archives,
            failure: None,
        })
    }

    fn write(&mut self, input: &FileRecord) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self
            .write_record(input)
            .map_err(|error| super::fields::sanitize(&error.to_string()));
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn write_record(&mut self, input: &FileRecord) -> Result<(), Box<dyn std::error::Error>> {
        let bytes = input.json.len() as u64 + 1;
        if self.size + bytes > self.limit {
            self.writer.rotate()?;
            self.size = 0;
        }
        let level = match input.record.level {
            super::LogLevel::Info => log::Level::Info,
            super::LogLevel::Warning => log::Level::Warn,
            super::LogLevel::Error => log::Level::Error,
        };
        self.writer.write(
            &mut DeferredNow::new(),
            &log::Record::builder()
                .level(level)
                .args(format_args!("{}", input.json))
                .build(),
        )?;
        self.size += bytes;
        if input.record.level == super::LogLevel::Error {
            self.writer.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.writer.flush().map_err(|error| error.to_string());
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn reopen(&mut self) -> Result<(), String> {
        *self = Self::open(self.directory.clone(), self.limit, self.archives)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn json_format(
    writer: &mut dyn Write,
    _: &mut DeferredNow,
    record: &log::Record<'_>,
) -> std::io::Result<()> {
    write!(writer, "{}", record.args())
}
