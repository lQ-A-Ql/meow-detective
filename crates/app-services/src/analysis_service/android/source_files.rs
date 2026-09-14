use domain::FileEntry;
use rusqlite::Connection;

use crate::analysis_service::candidates::find_candidate_by_path_suffix;
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::SourceReadContext;

pub(super) fn read_first_available_file(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    suffixes: &[&str],
    limit: usize,
    warnings: &mut Vec<String>,
    scanned_file_count: &mut u64,
) -> Result<Option<(FileEntry, Vec<u8>)>, AnalysisServiceError> {
    let Some(entry) = find_first_candidate(source_conn, suffixes)? else {
        return Ok(None);
    };
    if entry.encrypted {
        warnings.push(format!("{} is encrypted and was not read", entry.path));
        return Ok(None);
    }
    let Some(entry_size) = entry.size else {
        warnings.push(format!(
            "{} has no catalog size and was not read",
            entry.path
        ));
        return Ok(None);
    };
    if entry_size > limit as u64 {
        warnings.push(format!(
            "{} exceeds the {} byte analysis limit",
            entry.path, limit
        ));
        return Ok(None);
    }
    *scanned_file_count = scanned_file_count.saturating_add(1);
    match source_reader.read_file_header_by_id(&entry.id, limit) {
        Ok(bytes) if bytes.len() as u64 == entry_size => Ok(Some((entry, bytes))),
        Ok(bytes) => {
            warnings.push(format!(
                "{} read length mismatch: catalog={} bytes, read={} bytes",
                entry.path,
                entry_size,
                bytes.len()
            ));
            Ok(None)
        }
        Err(error) => {
            warnings.push(format!("{} could not be read: {error}", entry.path));
            Ok(None)
        }
    }
}

fn find_first_candidate(
    source_conn: &Connection,
    suffixes: &[&str],
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    for suffix in suffixes {
        if let Some(entry) = find_candidate_by_path_suffix(source_conn, suffix)? {
            return Ok(Some(entry));
        }
    }
    Ok(None)
}
