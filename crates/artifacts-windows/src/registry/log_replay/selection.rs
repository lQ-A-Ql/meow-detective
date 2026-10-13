use super::{
    header::{forward, Header},
    types::{ParsedLog, Record},
    LogDisposition, LogFormat, LogReplayError, LogReplayReport,
};

pub(super) fn select(
    base: &Header,
    logs: &[ParsedLog<'_>],
    report: &mut LogReplayReport,
) -> Result<Vec<usize>, LogReplayError> {
    let mut eligible = Vec::new();
    for (index, log) in logs.iter().enumerate() {
        if base.checksum_valid && !forward(log.header.primary, base.secondary) {
            if let Some(item) = report.logs.iter_mut().find(|r| r.slot == log.slot) {
                item.disposition = LogDisposition::Stale;
            }
        } else {
            eligible.push(index);
        }
    }
    if eligible.is_empty() {
        return Err(LogReplayError::NoLog);
    }
    if eligible.len() == 1 {
        return Ok(eligible);
    }
    let first = &logs[eligible[0]];
    let second = &logs[eligible[1]];
    if first.format != second.format {
        return Err(LogReplayError::Conflict);
    }
    if first.header.primary == second.header.primary {
        if !same_prefix(first, second) {
            return Err(LogReplayError::Conflict);
        }
        eligible.sort_by_key(|i| logs[*i].records.len());
        let duplicate = eligible.remove(0);
        report
            .logs
            .iter_mut()
            .find(|r| r.slot == logs[duplicate].slot)
            .expect("known log")
            .disposition = LogDisposition::Duplicate;
        return Ok(eligible);
    }
    let distance = first.header.primary.wrapping_sub(second.header.primary);
    if distance == 0x8000_0000 {
        return Err(LogReplayError::Conflict);
    }
    if distance < 0x8000_0000 {
        eligible.swap(0, 1);
    }
    if !base.checksum_valid || first.format == LogFormat::Dirt {
        return Ok(vec![eligible[1]]);
    }
    Ok(eligible)
}
fn same_prefix(left: &ParsedLog<'_>, right: &ParsedLog<'_>) -> bool {
    left.records
        .iter()
        .zip(&right.records)
        .all(|(a, b)| same_record(left, a, right, b))
}
fn same_record(left: &ParsedLog<'_>, a: &Record, right: &ParsedLog<'_>, b: &Record) -> bool {
    a.sequence == b.sequence
        && a.bins == b.bins
        && a.flags == b.flags
        && a.digest == b.digest
        && a.pages.len() == b.pages.len()
        && a.pages.iter().zip(&b.pages).all(|(p, q)| {
            p.offset == q.offset
                && p.length == q.length
                && left.data[p.data_start..p.data_start + p.length]
                    == right.data[q.data_start..q.data_start + q.length]
        })
}
