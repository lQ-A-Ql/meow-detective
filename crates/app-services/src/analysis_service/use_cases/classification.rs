use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::{AnalysisFileClassificationDto, FileClassificationBoardDto};

use super::source::open_ready_analysis_source;
use crate::analysis_service::file_classification::{
    build_file_classification_board, MAGIC_HEADER_BYTES,
};
use crate::analysis_service::{classify_files_by_metadata, AnalysisServiceError};
use crate::file_service::read_file_header_by_id;

pub fn classify_source_files(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    sample_size: u32,
) -> Result<Vec<AnalysisFileClassificationDto>, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    classify_files_by_metadata(&source.connection, sample_size)
}

/// Two-level classification board: magic-byte detection on the largest
/// `magic_read_limit` files, extension/path inference for the rest.
///
/// Emitted row ids are wrapped into the global `ds:<dataSourceId>:<localId>`
/// form so preview/viewer commands can resolve them through the source-scoped
/// file-id path.
pub fn get_file_classification_board(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    magic_read_limit: u32,
) -> Result<FileClassificationBoardDto, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    let mut board =
        build_file_classification_board(&source.connection, magic_read_limit, |file_id| {
            read_file_header_by_id(&source.connection, file_id, MAGIC_HEADER_BYTES)
        })?;
    for group in &mut board.groups {
        for subcategory in &mut group.subcategories {
            for file in &mut subcategory.files {
                file.file_id = crate::source_db::GlobalFileId::new(
                    data_source_id.clone(),
                    domain::FileEntryId(file.file_id.clone()),
                )
                .encode()
                .0;
            }
        }
    }
    Ok(board)
}
