use infrastructure::logging::{LogLevel, LogRecord, WriterHealth};
use transport::dto::{ApplicationLogLevelDto, ApplicationLogRecordDto, ApplicationLogWriterDto};

pub(super) fn level_dto(level: LogLevel) -> ApplicationLogLevelDto {
    match level {
        LogLevel::Info => ApplicationLogLevelDto::Info,
        LogLevel::Warning => ApplicationLogLevelDto::Warning,
        LogLevel::Error => ApplicationLogLevelDto::Error,
    }
}

pub(super) fn record(record: &LogRecord) -> ApplicationLogRecordDto {
    ApplicationLogRecordDto {
        session_id: record.session_id.clone(),
        sequence: record.sequence,
        timestamp: record.timestamp.clone(),
        level: level_dto(record.level),
        target: record.target.clone(),
        message: record.message.clone(),
        fields: record.fields.clone(),
        thread_id: record.thread_id.clone(),
        thread_name: record.thread_name.clone(),
        truncated: record.truncated,
    }
}

pub(super) fn writer(health: WriterHealth) -> ApplicationLogWriterDto {
    ApplicationLogWriterDto {
        failure: health.failure,
        failed_records: health.failed_records,
        queued_records: health.queued_records as u64,
        queued_bytes: health.queued_bytes as u64,
        written_sequence: health.written_sequence,
    }
}
