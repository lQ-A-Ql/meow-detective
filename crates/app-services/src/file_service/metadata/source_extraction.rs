use std::{path::Path, sync::Arc};

use domain::CaseId;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::dto::FileExtractionResultDto;

use crate::file_service::{FileServiceError, SourceReadContext};

use super::source_routing::open_source_for_file_id;

pub fn extract_file_to_destination_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<FileExtractionResultDto, FileServiceError> {
    extract_routed_file(ExtractionRouteRequest {
        bitlocker_runtime: None,
        case_conn,
        case_root,
        case_id,
        file_id,
        destination_path,
        overwrite,
        case_managed: false,
    })
}

pub fn extract_file_to_destination_for_case_with_bitlocker(
    bitlocker_runtime: &Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<FileExtractionResultDto, FileServiceError> {
    extract_routed_file(ExtractionRouteRequest {
        bitlocker_runtime: Some(bitlocker_runtime),
        case_conn,
        case_root,
        case_id,
        file_id,
        destination_path,
        overwrite,
        case_managed: false,
    })
}

pub(crate) fn extract_file_to_managed_destination_for_case(
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<FileExtractionResultDto, FileServiceError> {
    extract_routed_file(ExtractionRouteRequest {
        bitlocker_runtime,
        case_conn,
        case_root,
        case_id,
        file_id,
        destination_path,
        overwrite,
        case_managed: true,
    })
}

struct ExtractionRouteRequest<'a> {
    bitlocker_runtime: Option<&'a Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
    case_conn: &'a Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
    file_id: &'a str,
    destination_path: &'a Path,
    overwrite: bool,
    case_managed: bool,
}

fn extract_routed_file(
    request: ExtractionRouteRequest<'_>,
) -> Result<FileExtractionResultDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(
        request.case_conn,
        request.case_root,
        request.case_id,
        request.file_id,
    )?;
    let entry = FileRepo::new(&source_conn)
        .find_by_id(&global_id.local_id)?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;
    let mut context = SourceReadContext::new(
        &source_conn,
        request.case_conn,
        request.case_root,
        request.case_id,
        &global_id.data_source_id,
    );
    if let Some(runtime) = request.bitlocker_runtime {
        context = context.with_bitlocker_runtime(runtime.clone());
    }
    let destination_scope = if request.case_managed {
        crate::file_service::extraction::policy::DestinationScope::CaseManaged {
            case_conn: request.case_conn,
            case_root: request.case_root,
            case_id: request.case_id,
        }
    } else {
        crate::file_service::extraction::policy::DestinationScope::ExternalCase {
            case_conn: request.case_conn,
            case_root: request.case_root,
            case_id: request.case_id,
        }
    };
    crate::file_service::extraction::extract_source_file(
        &mut context,
        crate::file_service::extraction::SourceExtractionRequest {
            global_file_id: request.file_id,
            local_file_id: &global_id.local_id,
            source_size: entry.size,
            destination_path: request.destination_path,
            overwrite: request.overwrite,
            destination_scope,
        },
    )
}
