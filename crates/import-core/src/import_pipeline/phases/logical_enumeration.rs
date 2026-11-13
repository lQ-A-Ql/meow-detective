use evidence_core::{ArchiveFsReader, FileSystemReader, LogicalFsReader};

use crate::import_pipeline::context::ImportJobContext;
use evidence_access::file_service;
use transport::CommandError;

pub(super) fn enumerate_logical_source(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<file_service::EnumerationStats, CommandError> {
    let fs: Box<dyn FileSystemReader> = match ctx.import_config.kind {
        domain::DataSourceKind::LogicalDirectory => Box::new(
            LogicalFsReader::open(&ctx.import_config.source_path, &data_source.name)
                .map_err(CommandError::from_service_error)?,
        ),
        domain::DataSourceKind::LogicalArchive => Box::new(
            ArchiveFsReader::open(&ctx.import_config.source_path, &data_source.name)
                .map_err(CommandError::from_service_error)?,
        ),
        _ => return Err(CommandError::unsupported("unsupported logical source kind")),
    };
    let source_conn = ctx
        .source_conn
        .ok_or_else(|| CommandError::internal("source DB connection is not initialized"))?;
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        ctx.case_root,
        &data_source.id.0,
        &ctx.job_id.0,
    )
    .map_err(CommandError::from_service_error)?;
    let target = if layout.published_path().is_dir() {
        layout.published_path()
    } else {
        layout.build_path()
    };
    std::fs::create_dir_all(&target).map_err(CommandError::from_typed_service_error)?;
    let catalog = source_kernel::storage_runtime::open_source_read_write_at_path(target)
        .map_err(CommandError::from_service_error)?;
    let progress = |percent: u32| {
        super::super::emit::emit_job_progress(
            ctx.event_sink(),
            &ctx.job_id.0,
            25 + percent.min(100) * 45 / 100,
            "Enumerating logical source",
        );
    };
    let result = file_service::enumerate_filesystem_with_root_name_and_cancel_with_catalog(
        source_conn,
        &data_source.id,
        fs.as_ref(),
        None,
        Some(&progress),
        Some(ctx.options.cancel_token),
        &catalog,
    );
    if ctx.cancel_requested() {
        return Err(CommandError::cancelled(
            "Logical source enumeration cancelled",
        ));
    }
    result.map_err(CommandError::from_service_error)
}
