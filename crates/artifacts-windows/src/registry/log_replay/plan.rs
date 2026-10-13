use super::{
    header::Header,
    new_format, old_format, selection,
    types::{failure, LogReport, ParsedLog},
    HiveLog, LogDisposition, LogFormat, LogReplayError, LogReplayFailure, LogReplayPlan,
    LogReplayReport, RecoveryState,
};

pub fn prepare_log_replay<'a>(
    base: &[u8],
    logs: &[HiveLog<'a>],
    max_bytes: usize,
) -> Result<LogReplayPlan<'a>, LogReplayFailure> {
    let mut report = LogReplayReport {
        state: RecoveryState::Failed,
        base_primary: 0,
        base_secondary: 0,
        base_checksum_valid: false,
        result_sequence: None,
        applied_entries: 0,
        applied_pages: 0,
        logs: Vec::new(),
        diagnostics: Vec::new(),
    };
    let header = Header::read(base, max_bytes).map_err(|e| failure(e, report.clone()))?;
    if header.kind != 0 {
        return Err(failure(LogReplayError::Header, report));
    }
    report.base_primary = header.primary;
    report.base_secondary = header.secondary;
    report.base_checksum_valid = header.checksum_valid;
    if logs.len() > 2 || (logs.len() == 2 && logs[0].slot == logs[1].slot) {
        return Err(failure(LogReplayError::Conflict, report));
    }
    let mut parsed = Vec::new();
    for log in logs {
        let mut item = LogReport {
            slot: log.slot,
            format: None,
            disposition: LogDisposition::Unavailable,
            first_sequence: None,
            last_sequence: None,
            applied_entries: 0,
            applied_pages: 0,
            diagnostic: None,
        };
        match parse_log(log, &header, max_bytes) {
            Ok(data) => {
                item.format = Some(data.format);
                item.first_sequence = Some(data.header.primary);
                item.last_sequence = data.records.last().map(|r| r.sequence);
                item.diagnostic = data.tail;
                item.disposition = if !header.dirty() {
                    LogDisposition::NotNeeded
                } else {
                    LogDisposition::NotSelected
                };
                parsed.push(data);
            }
            Err(error) => {
                if log.bytes.is_some() && header.dirty() {
                    report.diagnostics.push(error);
                }
                item.disposition = if log.bytes.is_some() {
                    LogDisposition::Invalid
                } else {
                    LogDisposition::Unavailable
                };
                item.diagnostic = Some(error);
            }
        }
        report.logs.push(item);
    }
    let mut plan = LogReplayPlan {
        max_bins: header.bins,
        base: header,
        logs: parsed,
        order: Vec::new(),
        report,
    };
    if !plan.base.dirty() {
        plan.report.state = RecoveryState::NotNeeded;
        plan.report.result_sequence = Some(plan.base.primary);
        return Ok(plan);
    }
    plan.order = selection::select(&plan.base, &plan.logs, &mut plan.report)
        .map_err(|e| failure(e, plan.report.clone()))?;
    plan.max_bins = plan
        .order
        .iter()
        .flat_map(|i| plan.logs[*i].records.iter().map(|r| r.bins))
        .max()
        .unwrap_or(plan.base.bins)
        .max(plan.base.bins);
    Ok(plan)
}
fn parse_log<'a>(
    log: &HiveLog<'a>,
    base: &Header,
    max_bytes: usize,
) -> Result<ParsedLog<'a>, LogReplayError> {
    let data = log.bytes.ok_or(LogReplayError::Unavailable)?;
    let header = Header::read(data, max_bytes)?;
    if !header.checksum_valid || header.primary != header.secondary {
        return Err(LogReplayError::Checksum);
    }
    if !base.matches(&header) {
        return Err(LogReplayError::Identity);
    }
    let format = match header.kind {
        6 => LogFormat::Hvle,
        1 | 2 => LogFormat::Dirt,
        _ => return Err(LogReplayError::Unsupported),
    };
    if format == LogFormat::Dirt && header.timestamp != base.timestamp {
        return Err(LogReplayError::Identity);
    }
    let mut parsed = ParsedLog {
        slot: log.slot,
        data,
        header,
        format,
        records: Vec::new(),
        tail: None,
    };
    match format {
        LogFormat::Hvle => new_format::parse(&mut parsed, max_bytes)?,
        LogFormat::Dirt => old_format::parse(&mut parsed)?,
    }
    Ok(parsed)
}
