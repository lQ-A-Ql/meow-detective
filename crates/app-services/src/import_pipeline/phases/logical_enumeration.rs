use evidence_core::{ArchiveFsReader, FileSystemReader, LogicalFsReader};

use crate::file_service;
use crate::import_pipeline::context::ImportJobContext;

pub(super) fn enumerate_logical_source(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<file_service::EnumerationStats, persistence_sqlite::DbError> {
    let fs: Box<dyn FileSystemReader> = match ctx.import_config.kind {
        domain::DataSourceKind::LogicalDirectory => Box::new(LogicalFsReader::open(
            &ctx.import_config.source_path,
            &data_source.name,
        )?),
        domain::DataSourceKind::LogicalArchive => Box::new(ArchiveFsReader::open(
            &ctx.import_config.source_path,
            &data_source.name,
        )?),
        _ => {
            return Err(persistence_sqlite::DbError::System(
                "unsupported logical source kind".to_string(),
            ))
        }
    };
    let source_conn = ctx.source_conn.ok_or_else(|| {
        persistence_sqlite::DbError::System("source DB connection is not initialized".to_string())
    })?;
    file_service::enumerate_filesystem_with_root_name_and_cancel(
        source_conn,
        &data_source.id,
        fs.as_ref(),
        None,
        None::<&dyn Fn(u32)>,
        Some(ctx.options.cancel_token),
    )
}
