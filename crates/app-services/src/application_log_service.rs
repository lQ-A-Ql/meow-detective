use infrastructure::logging::{LogDirection, LogFilter, LogLevel, LogRuntime};
use serde::{Deserialize, Serialize};
use transport::{
    dto::*,
    paging::{decode_opaque_cursor, encode_opaque_cursor},
    CommandError,
};

mod mapping;

#[derive(Clone)]
pub struct ApplicationLogService {
    runtime: LogRuntime,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LogCursor {
    session_id: String,
    level: ApplicationLogLevelDto,
    filter: ApplicationLogFilterDto,
    direction: ApplicationLogDirectionDto,
    sequence: u64,
}

impl ApplicationLogService {
    pub fn new(runtime: LogRuntime) -> Self {
        Self { runtime }
    }

    pub fn read(
        &self,
        request: ApplicationLogRequestDto,
    ) -> Result<ApplicationLogSnapshotDto, CommandError> {
        validate(&request)?;
        let mut positions = [None; 3];
        for value in &request.cursors {
            let cursor: LogCursor = decode_opaque_cursor(value)
                .map_err(|_| CommandError::invalid_input("invalid application log cursor"))?;
            let index = match cursor.level {
                ApplicationLogLevelDto::Info => 0,
                ApplicationLogLevelDto::Warning => 1,
                ApplicationLogLevelDto::Error => 2,
            };
            if cursor.session_id != self.runtime.session_id()
                || cursor.filter != request.filter
                || cursor.direction != request.direction
                || positions[index].is_some()
            {
                return Err(CommandError::invalid_input(
                    "application log cursor context changed",
                ));
            }
            positions[index] = Some(cursor.sequence);
        }
        if request.direction != ApplicationLogDirectionDto::Latest && positions.contains(&None) {
            return Err(CommandError::invalid_input(
                "each log level requires a cursor",
            ));
        }
        let snapshot = self
            .runtime
            .read(infrastructure::logging::LogQuery {
                direction: match request.direction {
                    ApplicationLogDirectionDto::Latest => LogDirection::Latest,
                    ApplicationLogDirectionDto::Forward => LogDirection::Forward,
                    ApplicationLogDirectionDto::Backward => LogDirection::Backward,
                },
                positions,
                filter: LogFilter {
                    text: request.filter.text.clone(),
                    target: request.filter.target.clone(),
                    case_id: request.filter.case_id.clone(),
                    data_source_id: request.filter.data_source_id.clone(),
                    job_id: request.filter.job_id.clone(),
                },
                limit_per_level: request.limit_per_level as usize,
            })
            .map_err(|error| CommandError::invalid_input(error.to_string()))?;
        let pages = snapshot
            .pages
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                Ok(ApplicationLogPageDto {
                    level: mapping::level_dto(page.level),
                    records: page
                        .records
                        .iter()
                        .map(|record| mapping::record(record))
                        .collect(),
                    forward_cursor: self.cursor(
                        page.level,
                        page.forward,
                        &request.filter,
                        ApplicationLogDirectionDto::Forward,
                    )?,
                    backward_cursor: self.cursor(
                        page.level,
                        page.backward,
                        &request.filter,
                        ApplicationLogDirectionDto::Backward,
                    )?,
                    has_more: page.has_more,
                    gap: page.gap,
                    emitted: page.emitted,
                    evicted: page.evicted,
                    truncated: page.truncated,
                    file_dropped: snapshot.writer.dropped[index],
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?;
        Ok(ApplicationLogSnapshotDto {
            session_id: snapshot.session_id,
            upper_sequence: snapshot.upper_sequence,
            pages,
            writer: mapping::writer(snapshot.writer),
        })
    }

    pub fn record(
        &self,
        session_id: &str,
        sequence: u64,
    ) -> Result<ApplicationLogRecordDto, CommandError> {
        if session_id != self.runtime.session_id() {
            return Err(CommandError::invalid_input(
                "application log session changed",
            ));
        }
        self.runtime
            .record(sequence)
            .map(|record| mapping::record(&record))
            .map_err(|_| CommandError::invalid_input("application log record has expired"))
    }

    pub fn reopen_writer(&self) -> Result<ApplicationLogWriterDto, CommandError> {
        self.runtime
            .reopen_writer()
            .map_err(|_| CommandError::internal("application log writer could not reopen"))?;
        Ok(mapping::writer(self.runtime.health()))
    }

    fn cursor(
        &self,
        level: LogLevel,
        sequence: u64,
        filter: &ApplicationLogFilterDto,
        direction: ApplicationLogDirectionDto,
    ) -> Result<String, CommandError> {
        encode_opaque_cursor(&LogCursor {
            session_id: self.runtime.session_id().to_owned(),
            level: mapping::level_dto(level),
            filter: filter.clone(),
            direction,
            sequence,
        })
        .map_err(|_| CommandError::internal("application log cursor could not be encoded"))
    }
}

fn validate(request: &ApplicationLogRequestDto) -> Result<(), CommandError> {
    if request.limit_per_level == 0
        || request.limit_per_level > 100
        || request.cursors.len() > 3
        || request.cursors.iter().any(|cursor| cursor.len() > 4096)
        || [
            &request.filter.text,
            &request.filter.target,
            &request.filter.case_id,
            &request.filter.data_source_id,
            &request.filter.job_id,
        ]
        .iter()
        .any(|value| value.len() > 256)
    {
        return Err(CommandError::invalid_input(
            "application log request exceeds its budget",
        ));
    }
    Ok(())
}
