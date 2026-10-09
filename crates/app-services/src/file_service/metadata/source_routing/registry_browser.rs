//! Source-bound read-only Windows Registry browser.

use std::path::Path;
use std::sync::Arc;

use domain::CaseId;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::dto::{RegistryBrowserKeyDto, RegistryBrowserValueDto, RegistryOverlaySourceDto};

use crate::bitlocker_runtime::BitLockerUnlockRegistry;
use crate::file_service::{FileServiceError, SourceReadContext};

const MAX_REGISTRY_HIVE_BYTES: usize = 64 * 1024 * 1024;

pub fn browse_registry_key_for_case(
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    key_path: &str,
) -> Result<RegistryBrowserKeyDto, FileServiceError> {
    let (global, source_conn) =
        super::open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut context = SourceReadContext::new(
        &source_conn,
        case_conn,
        case_root,
        case_id,
        &global.data_source_id,
    )
    .with_bitlocker_runtime(bitlocker_runtime.clone());
    let handle =
        crate::file_service::viewer::open_file_handle_real(&mut context, &global.local_id.0)?;
    if handle.size > MAX_REGISTRY_HIVE_BYTES as u64 {
        return Err(FileServiceError::Unsupported(
            "registry hive exceeds the bounded browser read limit".to_string(),
        ));
    }
    let bytes = context.read_file_header_by_id(&global.local_id, MAX_REGISTRY_HIVE_BYTES)?;
    let overlay_warning = registry_overlay_warning(&source_conn, &global.local_id.0);
    let key = artifacts_windows::browse_registry_hive(&bytes, key_path)
        .map_err(FileServiceError::integrity)?;
    Ok(RegistryBrowserKeyDto {
        path: key.path,
        overlay_source: RegistryOverlaySourceDto::Base,
        overlay_warning: Some(overlay_warning),
        name: key.name,
        cell_offset: key.cell_offset,
        last_write_time: key.last_write_time,
        subkey_count: key.subkey_count,
        value_count: key.value_count,
        values: key
            .values
            .into_iter()
            .map(|value| RegistryBrowserValueDto {
                name: value.name,
                value_type: value.value_type,
                decoded: value.decoded,
                raw_hex: value.raw_hex,
                cell_offset: value.cell_offset,
            })
            .collect(),
        subkeys: key.subkeys,
    })
}

fn registry_overlay_warning(source_conn: &Connection, file_id: &str) -> String {
    let repo = FileRepo::new(source_conn);
    let has_transaction_logs = repo
        .find_by_id(&domain::FileEntryId(file_id.to_string()))
        .ok()
        .flatten()
        .and_then(|entry| entry.parent_id)
        .and_then(|parent| repo.find_children(&parent).ok())
        .is_some_and(|siblings| {
            siblings.iter().any(|entry| {
                let name = entry.name.to_ascii_lowercase();
                name.ends_with(".log1") || name.ends_with(".log2")
            })
        });
    if has_transaction_logs {
        "LOG1/LOG2 transaction logs detected but not merged in this browser view; use the registry extractor for recovered values".to_string()
    } else {
        "Browser view reads the base hive; no LOG1/LOG2 transaction logs were detected beside this hive".to_string()
    }
}
