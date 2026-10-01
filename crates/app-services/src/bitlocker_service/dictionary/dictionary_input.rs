use std::io::{self, BufRead};
use std::sync::atomic::{AtomicBool, Ordering};
use zeroize::Zeroizing;

use super::{
    BitLockerServiceError, MAX_DICTIONARY_BATCH_BYTES, MAX_DICTIONARY_BYTES,
    MAX_DICTIONARY_LINE_BYTES,
};

pub(crate) enum DictionaryBatch {
    Cancelled,
    Empty,
    End,
    Candidates(Vec<Zeroizing<String>>),
}

pub(crate) fn read_candidate_batch<R: BufRead>(
    reader: &mut R,
    cancel_token: &AtomicBool,
    bytes_processed: &mut u64,
    batch_size: usize,
) -> Result<DictionaryBatch, BitLockerServiceError> {
    let mut candidates = Vec::with_capacity(batch_size);
    let mut batch_bytes = 0usize;
    let mut at_end = false;
    while candidates.len() < batch_size
        && batch_bytes <= MAX_DICTIONARY_BATCH_BYTES - MAX_DICTIONARY_LINE_BYTES
    {
        if cancel_token.load(Ordering::Acquire) {
            return Ok(DictionaryBatch::Cancelled);
        }
        let Some(raw_line) = read_bounded_line(reader)? else {
            at_end = true;
            break;
        };
        *bytes_processed = checked_bytes_processed(*bytes_processed, raw_line.len())?;
        batch_bytes = batch_bytes.saturating_add(raw_line.len());
        let candidate = decode_candidate(raw_line)?;
        if !candidate.is_empty() {
            candidates.push(candidate);
        }
    }
    if candidates.is_empty() {
        Ok(if at_end {
            DictionaryBatch::End
        } else {
            DictionaryBatch::Empty
        })
    } else {
        Ok(DictionaryBatch::Candidates(candidates))
    }
}

pub(crate) fn checked_bytes_processed(
    current: u64,
    line_len: usize,
) -> Result<u64, BitLockerServiceError> {
    let additional =
        u64::try_from(line_len).map_err(|_| BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit",
        })?;
    let next = current
        .checked_add(additional)
        .ok_or(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit",
        })?;
    if next > MAX_DICTIONARY_BYTES {
        return Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit",
        });
    }
    Ok(next)
}

pub(crate) fn decode_candidate(
    raw_line: Zeroizing<Vec<u8>>,
) -> Result<Zeroizing<String>, BitLockerServiceError> {
    let mut raw_line = raw_line;
    if raw_line.last() == Some(&b'\n') {
        raw_line.pop();
        if raw_line.last() == Some(&b'\r') {
            raw_line.pop();
        }
    }
    let mut candidate = Zeroizing::new(
        std::str::from_utf8(&raw_line)
            .map_err(|_| BitLockerServiceError::DictionaryInvalid {
                reason: "dictionary contains invalid UTF-8",
            })?
            .to_owned(),
    );
    if candidate.starts_with('\u{feff}') {
        candidate.drain(..'\u{feff}'.len_utf8());
    }
    Ok(candidate)
}

pub(crate) fn read_bounded_line<R: BufRead>(
    reader: &mut R,
) -> Result<Option<Zeroizing<Vec<u8>>>, BitLockerServiceError> {
    let mut line = Zeroizing::new(Vec::new());
    loop {
        let chunk = reader
            .fill_buf()
            .map_err(BitLockerServiceError::DictionaryRead)?;
        if chunk.is_empty() {
            return Ok((!line.is_empty()).then(|| std::mem::take(&mut line)));
        }
        if let Some(newline) = chunk.iter().position(|byte| *byte == b'\n') {
            let take = newline + 1;
            if line.len().saturating_add(take) > MAX_DICTIONARY_LINE_BYTES {
                return Err(BitLockerServiceError::DictionaryInvalid {
                    reason: "dictionary line exceeds the size limit",
                });
            }
            line.extend_from_slice(&chunk[..take]);
            reader.consume(take);
            return Ok(Some(std::mem::take(&mut line)));
        }
        if line.len().saturating_add(chunk.len()) > MAX_DICTIONARY_LINE_BYTES {
            return Err(BitLockerServiceError::DictionaryInvalid {
                reason: "dictionary line exceeds the size limit",
            });
        }
        let consumed = chunk.len();
        line.extend_from_slice(chunk);
        reader.consume(consumed);
    }
}

impl From<io::Error> for BitLockerServiceError {
    fn from(error: io::Error) -> Self {
        Self::DictionaryRead(error)
    }
}
