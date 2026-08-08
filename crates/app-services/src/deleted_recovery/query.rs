use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryHashAlgorithm, DeletedRecoveryRepo,
};
use rusqlite::Connection;
use transport::dto::{
    DeletedRecoveryHashSearchDto, DeletedRecoveryPageDto, RecoveryHashAlgorithmDto,
};

use super::{mapping::page_to_dto, mapping::recovery_to_dto, DeletedRecoveryError};

const HASH_SEARCH_LIMIT: u32 = 100;

pub fn list_deleted_recoveries(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    offset: u64,
    limit: u32,
) -> Result<DeletedRecoveryPageDto, DeletedRecoveryError> {
    let source = crate::source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )?;
    let page = DeletedRecoveryRepo::new(&source.connection)
        .list_page(&data_source_id.0, partition_index, offset, limit)?
        .ok_or_else(|| DeletedRecoveryError::NotFound {
            data_source_id: data_source_id.0.clone(),
            partition_index,
        })?;
    page_to_dto(page)
}

pub fn search_deleted_recoveries_by_hash(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    algorithm: RecoveryHashAlgorithmDto,
    normalized_hash: &str,
) -> Result<DeletedRecoveryHashSearchDto, DeletedRecoveryError> {
    let source = crate::source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )?;
    let repo_algorithm = match algorithm {
        RecoveryHashAlgorithmDto::Md5 => DeletedRecoveryHashAlgorithm::Md5,
        RecoveryHashAlgorithmDto::Sha1 => DeletedRecoveryHashAlgorithm::Sha1,
        RecoveryHashAlgorithmDto::Sha256 => DeletedRecoveryHashAlgorithm::Sha256,
    };
    let matches = DeletedRecoveryRepo::new(&source.connection)
        .search_by_hash(
            &data_source_id.0,
            repo_algorithm,
            normalized_hash,
            HASH_SEARCH_LIMIT,
        )?
        .into_iter()
        .map(|(scan, recovery)| {
            recovery_to_dto(
                recovery,
                &scan.data_source_id,
                scan.partition_index,
                &scan.filesystem_type,
                scan.filesystem_uuid.as_deref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DeletedRecoveryHashSearchDto {
        algorithm,
        normalized_hash: normalized_hash.to_string(),
        matches,
    })
}
