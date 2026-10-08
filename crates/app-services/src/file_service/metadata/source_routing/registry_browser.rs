//! Source-bound read-only Windows Registry browser.

use std::path::Path;
use std::sync::Arc;

use domain::CaseId;
use rusqlite::Connection;
use transport::dto::{RegistryBrowserKeyDto, RegistryBrowserValueDto};

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
    let key = artifacts_windows::browse_registry_hive(&bytes, key_path)
        .map_err(FileServiceError::integrity)?;
    Ok(RegistryBrowserKeyDto {
        path: key.path,
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
