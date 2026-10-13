use super::{
    hash::marvin64,
    header::{forward, u32_at, u64_at},
    types::{Page, ParsedLog, Record},
    LogReplayError,
};

pub(super) fn parse(log: &mut ParsedLog<'_>, max_bytes: usize) -> Result<(), LogReplayError> {
    let mut at = 512;
    let mut expected = log.header.primary;
    while at < log.data.len() {
        let remaining = &log.data[at..];
        if remaining.iter().all(|b| *b == 0) {
            break;
        }
        if remaining.get(..4) != Some(b"HvLE") {
            break;
        } // Initialized journal slack is not an entry.
        if let Ok(sequence) = u32_at(remaining, 12) {
            if sequence != expected && forward(expected, sequence) {
                break;
            } // Older, validated-ring region is outside this journal prefix.
        }
        match entry(log.data, at, expected, max_bytes) {
            Ok((record, size)) => {
                log.records.push(record);
                at += size;
                expected = expected.wrapping_add(1);
            }
            Err(error) => {
                log.tail = Some(error);
                break;
            }
        }
        if log.records.len() > 65536 {
            return Err(LogReplayError::Bounds);
        }
    }
    if log.records.is_empty() {
        return Err(log.tail.unwrap_or(LogReplayError::Truncated));
    }
    Ok(())
}
fn entry(
    data: &[u8],
    at: usize,
    expected: u32,
    max_bytes: usize,
) -> Result<(Record, usize), LogReplayError> {
    let size = u32_at(data, at + 4)? as usize;
    if size < 512 || !size.is_multiple_of(512) {
        return Err(LogReplayError::Bounds);
    }
    let record = data
        .get(at..at.checked_add(size).ok_or(LogReplayError::Bounds)?)
        .ok_or(LogReplayError::Truncated)?;
    if marvin64(&record[40..]) != u64_at(record, 24)?
        || marvin64(&record[..32]) != u64_at(record, 32)?
    {
        return Err(LogReplayError::Hash);
    }
    let sequence = u32_at(record, 12)?;
    if sequence != expected {
        return Err(LogReplayError::Sequence);
    }
    let bins = u32_at(record, 16)? as usize;
    let flags = u32_at(record, 8)?;
    if bins < 4096
        || !bins.is_multiple_of(4096)
        || bins.checked_add(4096).is_none_or(|n| n > max_bytes)
        || flags > 1
    {
        return Err(LogReplayError::Bounds);
    }
    let count = u32_at(record, 20)? as usize;
    if count == 0 || count > 131072 {
        return Err(LogReplayError::Bounds);
    }
    let mut cursor = 40usize
        .checked_add(count * 8)
        .filter(|n| *n <= size)
        .ok_or(LogReplayError::Truncated)?;
    let mut pages = Vec::with_capacity(count);
    for index in 0..count {
        let offset = u32_at(record, 40 + index * 8)? as usize;
        let length = u32_at(record, 44 + index * 8)? as usize;
        if length == 0
            || !offset.is_multiple_of(512)
            || !length.is_multiple_of(512)
            || offset.checked_add(length).is_none_or(|n| n > bins)
        {
            return Err(LogReplayError::Bounds);
        }
        if cursor.checked_add(length).is_none_or(|n| n > size) {
            return Err(LogReplayError::Truncated);
        }
        pages.push(Page {
            offset,
            length,
            data_start: at + cursor,
        });
        cursor += length;
    }
    validate_ranges(&pages)?;
    Ok((
        Record {
            sequence,
            bins,
            flags,
            pages,
            digest: u64_at(record, 24)?,
        },
        size,
    ))
}
fn validate_ranges(pages: &[Page]) -> Result<(), LogReplayError> {
    let mut spans: Vec<_> = pages
        .iter()
        .map(|p| (p.offset, p.offset + p.length))
        .collect();
    spans.sort_unstable();
    if spans.windows(2).any(|p| p[0].1 > p[1].0) {
        return Err(LogReplayError::Bounds);
    }
    Ok(())
}
