use super::registry::MAX_HIVE_BYTES;
use artifacts_windows::registry::browser::{BrowserError, BrowserKey, BrowserValue, HiveSnapshot};
use domain::FileEntry;
use transport::dto::*;

pub(super) fn descriptor(entry: &FileEntry) -> RegistryHiveDescriptorDto {
    let parts: Vec<_> = entry.path.split(['/', '\\']).collect();
    let partition_index = parts
        .first()
        .and_then(|p| p.strip_prefix("[P"))
        .and_then(|p| p.strip_suffix(']'))
        .and_then(|p| p.parse().ok());
    let profile_label = parts
        .windows(2)
        .find(|p| p[0].eq_ignore_ascii_case("Users"))
        .map(|p| p[1].to_string());
    RegistryHiveDescriptorDto {
        file_id: crate::source_db::encode_source_scoped_id(&entry.data_source_id, &entry.id.0),
        data_source_id: entry.data_source_id.0.clone(),
        name: entry.name.clone(),
        evidence_path: entry.path.clone(),
        byte_length: entry.size.unwrap_or(0),
        partition_index,
        profile_label,
        read_state: if entry.encrypted {
            RegistryHiveReadStateDto::Locked
        } else if entry.size.unwrap_or(0) > MAX_HIVE_BYTES as u64 {
            RegistryHiveReadStateDto::Unsupported
        } else {
            RegistryHiveReadStateDto::Candidate
        },
    }
}
pub(super) fn key(key: &BrowserKey, id: String) -> RegistryKeyMetadataDto {
    RegistryKeyMetadataDto {
        key_id: id,
        path: key.path.clone(),
        name: key.name.clone(),
        cell_offset: key.offset,
        subkey_count: key.subkey_count,
        value_count: key.value_count,
        read_state: RegistryItemReadStateDto::Readable,
        overlay_source: overlay_source(key.overlay_source),
        last_write_filetime: key.filetime.map(|v| v.to_string()),
    }
}
pub(super) fn corrupt_key(offset: u32, id: String) -> RegistryKeyMetadataDto {
    RegistryKeyMetadataDto {
        key_id: id,
        path: String::new(),
        name: format!("[0x{offset:08X}]"),
        cell_offset: offset,
        subkey_count: 0,
        value_count: 0,
        read_state: RegistryItemReadStateDto::Corrupt,
        overlay_source: RegistryOverlaySourceDto::Base,
        last_write_filetime: None,
    }
}
pub(super) fn value(value: BrowserValue, id: String) -> RegistryValueMetadataDto {
    RegistryValueMetadataDto {
        value_id: id,
        name: value.name,
        type_code: value.type_code,
        type_name: HiveSnapshot::value_type_name(value.type_code),
        byte_length: value.byte_length as u64,
        preview: value.preview,
        cell_offset: value.offset,
        read_state: if value.data_error.is_some() {
            RegistryItemReadStateDto::Corrupt
        } else if value.byte_length > 256 {
            RegistryItemReadStateDto::Deferred
        } else {
            RegistryItemReadStateDto::Readable
        },
        overlay_source: overlay_source(value.overlay_source),
        data_overlay_source: overlay_source(value.data_overlay_source),
    }
}
pub(super) fn corrupt_value(offset: u32, id: String) -> RegistryValueMetadataDto {
    RegistryValueMetadataDto {
        value_id: id,
        name: format!("[0x{offset:08X}]"),
        type_code: u32::MAX,
        type_name: "REG_UNKNOWN".into(),
        byte_length: 0,
        preview: String::new(),
        cell_offset: offset,
        read_state: RegistryItemReadStateDto::Corrupt,
        overlay_source: RegistryOverlaySourceDto::Base,
        data_overlay_source: RegistryOverlaySourceDto::Base,
    }
}
fn overlay_source(
    source: artifacts_windows::registry::browser::BrowserOverlaySource,
) -> RegistryOverlaySourceDto {
    match source {
        artifacts_windows::registry::browser::BrowserOverlaySource::Base => {
            RegistryOverlaySourceDto::Base
        }
        artifacts_windows::registry::browser::BrowserOverlaySource::Recovered => {
            RegistryOverlaySourceDto::Recovered
        }
        artifacts_windows::registry::browser::BrowserOverlaySource::Merged => {
            RegistryOverlaySourceDto::Merged
        }
    }
}
pub(super) fn diagnostic(code: &str, id: Option<String>) -> RegistryBrowserDiagnosticDto {
    RegistryBrowserDiagnosticDto {
        code: code.to_string(),
        recoverable: true,
        item_id: id,
    }
}
pub(super) fn coverage(
    declared: u64,
    returned: usize,
    skipped: u32,
    partial: bool,
) -> RegistryBrowserCoverageDto {
    RegistryBrowserCoverageDto {
        declared_count: declared,
        returned_count: returned as u32,
        skipped_count: skipped,
        status: if partial {
            RegistryBrowserCoverageStatusDto::Partial
        } else {
            RegistryBrowserCoverageStatusDto::Complete
        },
    }
}
pub(super) fn list_diagnostics(error: Option<BrowserError>) -> Vec<RegistryBrowserDiagnosticDto> {
    error
        .map(|_| vec![diagnostic("corruptList", None)])
        .unwrap_or_default()
}
pub(super) fn page_limit(limit: u32) -> Result<usize, super::RegistryBrowserError> {
    if !(1..=200).contains(&limit) {
        return Err(super::RegistryBrowserError::InvalidRequest);
    }
    Ok(limit as usize)
}
