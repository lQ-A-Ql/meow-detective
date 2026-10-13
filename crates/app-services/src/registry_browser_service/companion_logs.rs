use super::{io::read_snapshot, RegistryBrowserError};
use crate::file_service::SourceReadContext;
use domain::FileEntry;
use sha2::{Digest, Sha256};
use transport::dto::*;

pub(super) const MAX_LOG_BYTES: usize = 64 * 1024 * 1024;
pub(super) struct LogSource {
    pub metadata: RegistryLogProvenanceDto,
    pub entry: Option<FileEntry>,
    pub bytes: Option<Vec<u8>>,
}
pub(super) fn sources(entries: Vec<FileEntry>, name: &str) -> Vec<LogSource> {
    [
        (RegistryLogSlotDto::Log1, "LOG1"),
        (RegistryLogSlotDto::Log2, "LOG2"),
    ]
    .into_iter()
    .map(|(slot, suffix)| {
        let matches: Vec<_> = entries
            .iter()
            .filter(|e| e.name.eq_ignore_ascii_case(&format!("{name}.{suffix}")))
            .collect();
        let entry = if matches.len() == 1 {
            Some(matches[0].clone())
        } else {
            None
        };
        let status = if matches.len() > 1 {
            RegistryLogStatusDto::Ambiguous
        } else if entry.as_ref().is_some_and(|e| e.encrypted) {
            RegistryLogStatusDto::Locked
        } else if entry
            .as_ref()
            .is_some_and(|e| e.size.unwrap_or(0) > MAX_LOG_BYTES as u64)
        {
            RegistryLogStatusDto::TooLarge
        } else if entry.is_none() {
            RegistryLogStatusDto::Missing
        } else {
            RegistryLogStatusDto::NotSelected
        };
        let metadata = RegistryLogProvenanceDto {
            slot,
            status,
            file_id: entry
                .as_ref()
                .map(|e| crate::source_db::encode_source_scoped_id(&e.data_source_id, &e.id.0)),
            evidence_path: entry.as_ref().map(|e| e.path.clone()),
            byte_length: entry.as_ref().and_then(|e| e.size),
            sha256: None,
            format: None,
            first_sequence: None,
            last_sequence: None,
            applied_entries: 0,
            applied_pages: 0,
            diagnostic_code: None,
        };
        LogSource {
            metadata,
            entry,
            bytes: None,
        }
    })
    .collect()
}
pub(super) fn sizes(context: &mut SourceReadContext<'_>, sources: &mut [LogSource]) -> usize {
    let mut total = 0;
    for log in sources {
        if log.metadata.status != RegistryLogStatusDto::NotSelected {
            continue;
        }
        let entry = log.entry.as_ref().expect("selected log entry");
        if entry.size == Some(0) {
            continue;
        }
        match crate::file_service::open_file_handle_real(&mut *context, &entry.id.0) {
            Ok(handle) if handle.size <= MAX_LOG_BYTES as u64 => {
                log.metadata.byte_length = Some(handle.size);
                total += handle.size as usize;
            }
            Ok(_) => log.metadata.status = RegistryLogStatusDto::TooLarge,
            Err(_) => log.metadata.status = RegistryLogStatusDto::Unavailable,
        }
    }
    total
}
pub(super) fn load(
    context: &mut SourceReadContext<'_>,
    sources: &mut [LogSource],
) -> Result<u64, RegistryBrowserError> {
    let mut count = 0;
    for log in sources {
        if log.metadata.status != RegistryLogStatusDto::NotSelected {
            continue;
        }
        let entry = log.entry.as_ref().expect("selected log entry");
        let size = log.metadata.byte_length.unwrap_or(0) as usize;
        match read_snapshot(context, &entry.id, size, size) {
            Ok(bytes) => {
                log.metadata.sha256 = Some(hex::encode(Sha256::digest(&bytes)));
                log.bytes = Some(bytes);
                count += 1;
            }
            Err(_) => log.metadata.status = RegistryLogStatusDto::Unavailable,
        }
    }
    Ok(count)
}
