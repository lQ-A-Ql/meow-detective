use domain::FileEntry;
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
};
use transport::{dto::ContentIndexSummaryDto, CommandError};
const MAX_TEXT: usize = 1024 * 1024;
pub(super) fn read(
    context: &mut crate::file_service::SourceReadContext<'_>,
    entry: &FileEntry,
    cancel: &AtomicBool,
) -> Result<(&'static str, String), CommandError> {
    if !entry.is_file() {
        return Ok(("notApplicable", String::new()));
    }
    if entry.size.is_some_and(|size| size > MAX_TEXT as u64) {
        return Ok(("oversized", String::new()));
    }
    if entry.encrypted {
        return Ok(("unreadable", String::new()));
    }
    let Ok(mut reader) = crate::file_service::open_file_content_by_id(context, &entry.id) else {
        return Ok(("unreadable", String::new()));
    };
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(CommandError::cancelled("Content indexing cancelled"));
        }
        let amount = match reader.read(&mut buffer) {
            Ok(amount) => amount,
            Err(_) => return Ok(("unreadable", String::new())),
        };
        if amount == 0 {
            break;
        }
        if bytes.len() + amount > MAX_TEXT {
            return Ok(("oversized", String::new()));
        }
        bytes.extend_from_slice(&buffer[..amount]);
    }
    if entry.size.is_some_and(|size| size != bytes.len() as u64) {
        return Ok(("unreadable", String::new()));
    }
    let text = search::extract_text(std::io::Cursor::new(bytes), &entry.id.0, None);
    let status = if text.extractable {
        "indexed"
    } else if text.encoding == "binary" {
        "binary"
    } else {
        "unsupportedEncoding"
    };
    Ok((status, text.content))
}
pub(super) fn count(summary: &mut ContentIndexSummaryDto, status: &str) {
    match status {
        "indexed" => summary.indexed += 1,
        "binary" => summary.binary += 1,
        "oversized" => summary.oversized += 1,
        "unreadable" => summary.unreadable += 1,
        "unsupportedEncoding" => summary.unsupported_encoding += 1,
        _ => summary.not_applicable += 1,
    }
}
pub(super) fn processed(summary: &ContentIndexSummaryDto) -> u64 {
    summary.indexed
        + summary.binary
        + summary.oversized
        + summary.unreadable
        + summary.unsupported_encoding
        + summary.not_applicable
}
