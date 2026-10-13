use super::{
    types::{Page, ParsedLog, Record},
    LogReplayError,
};

pub(super) fn parse(log: &mut ParsedLog<'_>) -> Result<(), LogReplayError> {
    let vector_length = log.header.bins / 4096;
    if log.data.get(512..516) != Some(b"DIRT") {
        return Err(LogReplayError::Unsupported);
    }
    let bitmap = log
        .data
        .get(516..516 + vector_length)
        .ok_or(LogReplayError::Truncated)?;
    let mut data_start = (516 + vector_length).div_ceil(512) * 512;
    let mut pages = Vec::new();
    for (byte_index, byte) in bitmap.iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) == 0 {
                continue;
            }
            let offset = (byte_index * 8 + bit) * 512;
            if data_start
                .checked_add(512)
                .is_none_or(|end| end > log.data.len())
            {
                return Err(LogReplayError::Truncated);
            }
            pages.push(Page {
                offset,
                length: 512,
                data_start,
            });
            data_start += 512;
        }
    }
    if pages.is_empty() || pages.len() > 131072 {
        return Err(LogReplayError::Bounds);
    }
    log.records.push(Record {
        sequence: log.header.primary,
        bins: log.header.bins,
        flags: 0,
        pages,
        digest: 0,
    });
    Ok(())
}
